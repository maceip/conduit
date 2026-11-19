//
//  ConduitMailCompose.swift
//  ConduitMail
//
//  The compose session handler.
//
//  Mail calls the handler when the user opens a compose window, and again
//  when the window closes.  The handler publishes the message that is
//  being written — its subject, its recipients, an excerpt of its body —
//  and changes nothing about it.
//

import Foundation
import MailKit

@MainActor
final class ConduitMailCompose: NSObject, MEComposeSessionHandler {

    /// The schema of a mail message.
    static let schema = "dev.conduit.mail.message/v1"

    /// How many characters of the body are published.
    static let excerptLimit = 280

    /// The near end the message goes to.
    private let nearEnd = NearEnd()

    // MARK: - MEComposeSessionHandler

    /// The user has opened a compose window.
    func mailComposeSessionDidBegin(_ session: MEComposeSession) {
        publish(session)
    }

    /// The compose window has closed.
    func mailComposeSessionDidEnd(_ session: MEComposeSession) {
        publish(session)
    }

    // MARK: - The publishing

    /// Publish the message being composed.
    private func publish(_ session: MEComposeSession) {
        let message = session.mailMessage
        let payload: [String: Any] = [
            "kind": "compose",
            "subject": message.subject,
            "recipients": ConduitMailCompose.addresses(message.allRecipientAddresses),
            "bodyExcerpt": ConduitMailCompose.excerpt(of: message.rawData,
                                                      limit: ConduitMailCompose.excerptLimit),
            "sessionID": session.sessionID.uuidString,
            "isReply": session.composeContext.originalMessage != nil,
        ]
        do {
            try nearEnd.publish(schema: ConduitMailCompose.schema, payload: payload)
        } catch {
            NSLog("conduit: the compose session was not published: %@", String(describing: error))
        }
    }

    /// The addresses of a list of recipients.
    ///
    /// - Parameter addresses: the addresses Mail knows.
    /// - Returns: the addresses as plain strings.
    static func addresses(_ addresses: [MEEmailAddress]) -> [String] {
        addresses.map { $0.addressString ?? $0.rawString }
    }

    /// The first lines of the body of a message, if Mail has built it.
    ///
    /// The raw data of a message is its headers, an empty line, and then
    /// its body.  The body is what follows the first empty line; when
    /// Mail has not built the message yet there is no body to publish.
    ///
    /// - Parameters:
    ///   - raw: the raw data of the message.
    ///   - limit: how many characters to publish.
    /// - Returns: the excerpt.
    static func excerpt(of raw: Data?, limit: Int) -> String {
        guard let raw, !raw.isEmpty else {
            return ""
        }
        let text = String(decoding: raw, as: UTF8.self)
        var body = text
        if let separator = text.range(of: "\r\n\r\n") {
            body = String(text[separator.upperBound...])
        } else if let separator = text.range(of: "\n\n") {
            body = String(text[separator.upperBound...])
        }
        let trimmed = body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.count > limit else {
            return trimmed
        }
        return String(trimmed.prefix(limit)) + "…"
    }
}
