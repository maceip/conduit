//
//  NearEnd.swift
//  ConduitShortcuts
//
//  The client of the Conduit near end: the half of the surface that lives
//  inside the app.
//
//  The near end is a separate process.  It holds the socket to the broker
//  and does the sealing.  This file speaks to it over a Unix socket, in
//  plain JSON, one message at a time:
//
//      plugin  ->  {"op":"publish","schema":"dev.conduit.shortcuts.state/v1","payload":{…}}
//      near end ->  {"op":"status","surface":"…","agreed":true,"connected":true,"seq":12}
//
//  A message on that socket is a four-byte big-endian length, and then
//  that many bytes of JSON.
//
//  What this file publishes is plaintext on that socket.  What leaves the
//  machine is not: the near end seals it under the key of the tunnel, and
//  the broker never sees it open.  This file holds no key and opens no
//  socket to the broker.
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

    /// Whether the two ends have agreed their keys.
    ///
    /// The near end cannot seal anything until they have, so nothing is
    /// published before this is true.
    private var agreed = false

    /// The last thing the near end complained about.
    private var complaint: String?

    /// How long a publish waits for the tunnel to agree, in seconds.
    private let agreementTimeout: TimeInterval

    /// The socket of the near end, from wherever the user put it.
    ///
    /// The environment variable first, then the shared defaults of the app
    /// group, and then the default.
    static func defaultSocketPath() -> String {
        if let path = ProcessInfo.processInfo.environment["CONDUIT_SOCKET"], !path.isEmpty {
            return path
        }
        if let path = UserDefaults(suiteName: "group.dev.conduit.shortcuts")?
            .string(forKey: "nearEndSocket"), !path.isEmpty {
            return path
        }
        return "/tmp/conduit-shortcuts.sock"
    }

    /// A client of the near end.
    ///
    /// - Parameters:
    ///   - socketPath: the path of the socket of the near end.
    ///   - agreementTimeout: how long a publish waits for the tunnel to
    ///     agree its keys, in seconds.
    init(socketPath: String = NearEnd.defaultSocketPath(),
         agreementTimeout: TimeInterval = 10) {
        self.socketPath = socketPath
        self.agreementTimeout = agreementTimeout
    }

    /// Whether the two ends have agreed their keys.
    var isAgreed: Bool {
        lock.lock()
        defer { lock.unlock() }
        return agreed
    }

    /// The last thing the near end complained about.
    var lastComplaint: String? {
        lock.lock()
        defer { lock.unlock() }
        return complaint
    }

    /// Open the socket to the near end.
    ///
    /// Opening it twice is not a mistake: the action publishes more than
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
    /// The near end cannot seal anything until the two ends have agreed
    /// their keys, and a publish before that is refused.  So the publish
    /// waits for the tunnel to agree, with a timeout.
    ///
    /// - Parameters:
    ///   - schema: the schema of the payload.
    ///   - payload: the payload, as JSON.
    func publish(schema: String, payload: [String: Any]) throws {
        try open()
        guard try waitUntilAgreed() else {
            throw Failure.notAgreed
        }
        try send(["op": "publish", "schema": schema, "payload": payload])
    }

    /// Ask how the tunnel is.
    func status() throws {
        try open()
        try send(["op": "status"])
    }

    /// Wait until the tunnel has a key.
    ///
    /// The two ends agree their keys when they have said hello to each
    /// other, and that takes a moment.  The near end answers a status with
    /// the state of the tunnel, so the question is asked again and again
    /// until the answer is yes, or until the wait is over.
    ///
    /// - Parameter timeout: how long to wait, in seconds.
    /// - Returns: whether the tunnel agreed.
    @discardableResult
    func waitUntilAgreed(timeout: TimeInterval? = nil) throws -> Bool {
        let deadline = Date().addingTimeInterval(timeout ?? agreementTimeout)
        while Date() < deadline {
            if isAgreed {
                return true
            }
            try send(["op": "status"])
            Thread.sleep(forTimeInterval: 0.05)
        }
        return isAgreed
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
        switch message["op"] as? String {
        case "status":
            if (message["agreed"] as? Bool) == true {
                agreed = true
            }
        case "error":
            complaint = message["message"] as? String
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
        /// The tunnel did not agree its keys in time.
        case notAgreed

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
            case .notAgreed:
                return "the near end did not agree the keys of the tunnel"
            }
        }
    }
}
