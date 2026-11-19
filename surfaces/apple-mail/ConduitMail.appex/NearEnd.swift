//
//  NearEnd.swift
//  ConduitMail
//
//  The client of the Conduit near end: the half of the surface that lives
//  inside the app.
//
//  The near end is a separate process.  It holds the socket to the broker
//  and does the sealing.  This file speaks to it over a Unix socket, in
//  plain JSON, one message at a time:
//
//      plugin  ->  {"t":"publish","s":"dev.conduit.mail.message/v1","c":"messages","p":{…}}
//      near end ->  {"t":"publish","s":"…","c":"…","seq":12,"p":{…}}
//
//  A message on that socket is a four-byte big-endian length, and then
//  that many bytes of JSON.
//
//  What this file publishes is plaintext on that socket.  What leaves the
//  machine is not: the near end seals it under the key of the tunnel, and
//  the broker never sees it open.  This file holds no key and opens no
//  socket to the broker.
//
//  There is no second vocabulary: the messages on that socket are the
//  frames of the tunnel.  And this file does not wait for the tunnel to
//  be up — the near end holds what is published until there is a key to
//  seal it under, and sends it then.
//

import Foundation
import Darwin

/// The client of a near end over a Unix socket.
final class NearEnd: @unchecked Sendable {

    /// The path of the socket of the near end.
    private let socketPath: String

    /// The socket itself, or -1 when it is not open.
    private var handle: Int32 = -1

    /// The lock of the socket: the reader thread and the publisher share it.
    private let lock = NSLock()

    /// The bytes that have come in but do not make a message yet.
    private var incoming = Data()

    /// Whether the tunnel is up.
    ///
    /// The near end says so with the welcome frame.  Nothing waits for
    /// it: the near end holds what is published until there is a key to
    /// seal it under, and sends it then.
    private var up = false

    /// The last thing the near end complained about.
    private var complaint: String?

    /// The channel this plugin publishes on.
    private let channel: String

    /// The socket of the near end, from wherever the user put it.
    ///
    /// The extension is sandboxed, so the path has to be one the extension
    /// can open: the environment variable first, then the shared defaults
    /// of the app group, and then the default.
    static func defaultSocketPath() -> String {
        if let path = ProcessInfo.processInfo.environment["CONDUIT_SOCKET"], !path.isEmpty {
            return path
        }
        if let path = UserDefaults(suiteName: "group.dev.conduit.mail")?
            .string(forKey: "nearEndSocket"), !path.isEmpty {
            return path
        }
        return "/tmp/conduit-mail.sock"
    }

    /// A client of the near end.
    ///
    /// - Parameters:
    ///   - socketPath: the path of the socket of the near end.
    ///   - channel: the channel this plugin publishes on.
    init(socketPath: String = NearEnd.defaultSocketPath(),
         channel: String = "messages") {
        self.socketPath = socketPath
        self.channel = channel
    }

    /// Whether the tunnel is up.
    var isUp: Bool {
        lock.lock()
        defer { lock.unlock() }
        return up
    }

    /// The last thing the near end complained about.
    var lastComplaint: String? {
        lock.lock()
        defer { lock.unlock() }
        return complaint
    }

    /// Open the socket to the near end.
    ///
    /// Opening it twice is not a mistake: the extension publishes more than
    /// once, and the socket is opened once.
    func open() throws {
        lock.lock()
        defer { lock.unlock() }
        if handle >= 0 {
            return
        }

        let file = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        guard file >= 0 else {
            throw Failure.socketFailed(errno)
        }

        var address = sockaddr_un()
        address.sun_family = sa_family_t(AF_UNIX)
        let path = Array(socketPath.utf8)
        let capacity = MemoryLayout.size(ofValue: address.sun_path)
        guard path.count < capacity else {
            Darwin.close(file)
            throw Failure.pathTooLong
        }
        withUnsafeMutableBytes(of: &address.sun_path) { raw in
            for (index, byte) in path.enumerated() {
                raw[index] = byte
            }
            raw[path.count] = 0
        }

        let connected = withUnsafePointer(to: &address) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
                Darwin.connect(file, sockaddrPointer, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard connected == 0 else {
            let code = errno
            Darwin.close(file)
            throw Failure.connectFailed(code)
        }

        handle = file
        Thread.detachNewThread { [weak self] in
            self?.readLoop()
        }
    }

    /// Close the socket.
    func close() {
        lock.lock()
        let file = handle
        handle = -1
        lock.unlock()
        if file >= 0 {
            Darwin.close(file)
        }
    }

    /// Publish what the app knows.
    ///
    /// The frame is the frame of the tunnel, with a plaintext payload: the
    /// near end seals it, and the broker never sees it open.  The publish
    /// does not wait for the tunnel to be up; the near end holds it until
    /// there is a key to seal it under.
    ///
    /// - Parameters:
    ///   - schema: the schema of the payload.
    ///   - payload: the payload, as JSON.
    func publish(schema: String, payload: [String: Any]) throws {
        try open()
        try send(["t": "publish", "s": schema, "c": channel, "seq": 0, "p": payload])
    }

    /// Say where the broker is.
    ///
    /// - Parameters:
    ///   - broker: the address of the broker.
    ///   - secret: the secret of the grant.
    ///   - instance: the instance of the tunnel.
    func connect(broker: String, secret: String, instance: String) throws {
        try open()
        try send([
            "t": "connect",
            "s": "",
            "c": channel,
            "seq": 0,
            "p": ["broker": broker, "secret": secret, "instance": instance],
        ])
    }

    /// Say something to the near end: a length, then the bytes.
    private func send(_ message: [String: Any]) throws {
        let body = try JSONSerialization.data(withJSONObject: message, options: [])

        var frame = Data()
        var length = UInt32(body.count).bigEndian
        withUnsafeBytes(of: &length) { raw in
            frame.append(contentsOf: raw)
        }
        frame.append(body)

        lock.lock()
        let file = handle
        lock.unlock()
        guard file >= 0 else {
            throw Failure.notOpen
        }

        try frame.withUnsafeBytes { raw in
            var sent = 0
            while sent < raw.count {
                let written = Darwin.write(file, raw.baseAddress!.advanced(by: sent), raw.count - sent)
                if written < 0 {
                    if errno == EINTR {
                        continue
                    }
                    throw Failure.writeFailed(errno)
                }
                sent += written
            }
        }
    }

    /// Read what the near end says, until it stops saying anything.
    private func readLoop() {
        let file = handle
        var chunk = [UInt8](repeating: 0, count: 4096)
        while true {
            let count = chunk.withUnsafeMutableBytes { raw in
                Darwin.read(file, raw.baseAddress, raw.count)
            }
            if count > 0 {
                receive(Data(chunk[0..<count]))
                continue
            }
            if count == 0 {
                // The near end closed the socket.
                break
            }
            if errno == EINTR {
                continue
            }
            break
        }
        close()
    }

    /// Take the messages out of what has come in.
    private func receive(_ data: Data) {
        lock.lock()
        incoming.append(data)
        var messages: [[String: Any]] = []
        while incoming.count >= 4 {
            let length = incoming.prefix(4).reduce(UInt32(0)) { ($0 << 8) | UInt32($1) }
            guard incoming.count >= 4 + Int(length) else {
                break
            }
            let body = incoming.subdata(in: 4..<(4 + Int(length)))
            incoming.removeSubrange(0..<(4 + Int(length)))
            if let object = try? JSONSerialization.jsonObject(with: body, options: []),
               let message = object as? [String: Any] {
                messages.append(message)
            }
        }
        lock.unlock()

        for message in messages {
            note(message)
        }
    }

    /// Remember what the near end said.
    private func note(_ message: [String: Any]) {
        lock.lock()
        defer { lock.unlock() }
        switch message["t"] as? String {
        case "welcome":
            // The broker welcomed the near end: the tunnel is up.
            up = true
        case "error":
            let payload = message["p"] as? [String: Any]
            complaint = payload?["message"] as? String
        default:
            break
        }
    }

    /// What can go wrong.
    enum Failure: Error, CustomStringConvertible {
        /// The socket could not be made.
        case socketFailed(Int32)
        /// The near end was not there.
        case connectFailed(Int32)
        /// The path of the socket is too long for the system.
        case pathTooLong
        /// The socket is not open.
        case notOpen
        /// The message could not be written.
        case writeFailed(Int32)

        var description: String {
            switch self {
            case .socketFailed(let code):
                return "the socket could not be made (\(code))"
            case .connectFailed(let code):
                return "the near end did not answer (\(code))"
            case .pathTooLong:
                return "the path of the socket is too long"
            case .notOpen:
                return "the socket to the near end is not open"
            case .writeFailed(let code):
                return "the message could not be written (\(code))"
            }
        }
    }
}
