//
//  ConduitMailAction.swift
//  ConduitMail
//
//  The message action handler.
//
//  Mail calls the handler for every message it downloads, before the
//  message reaches the inbox.  The handler publishes what the message
//  says — its subject, who it is from, when it came — and takes no action
//  on the message itself.
//

import Foundation
import MailKit

@MainActor
final class ConduitMailAction: NSObject, MEMessageActionHandler {

    /// The schema of a mail message.
    static let schema = "dev.conduit.mail.message/v1"

    /// The near end the message goes to.
    private let nearEnd = NearEnd()

    // MARK: - MEMessageActionHandler

    /// Mail has downloaded a message and wants to know what to do with it.
    ///
    /// Conduit takes no action on the message: the decision is nil, which
    /// tells Mail to leave the message exactly as it found it.
    func decideAction(for message: MEMessage,
                      completionHandler: @escaping @Sendable (MEMessageActionDecision?) -> Void) {
        publish(message)
        completionHandler(nil)
    }

    // MARK: - The publishing

    /// Publish the message Mail is downloading.
    private func publish(_ message: MEMessage) {
        let payload: [String: Any] = [
            "kind": "action",
            "subject": message.subject,
            "sender": ConduitMailAction.address(message.fromAddress),
            "recipients": ConduitMailCompose.addresses(message.allRecipientAddresses),
            "date": ConduitMailAction.timestamp(message.dateSent ?? message.dateReceived),
        ]
        do {
            try nearEnd.publish(schema: ConduitMailAction.schema, payload: payload)
        } catch {
            NSLog("conduit: the message was not published: %@", String(describing: error))
        }
    }

    /// The address of one recipient.
    ///
    /// - Parameter address: the address Mail knows.
    /// - Returns: the address as a plain string.
    static func address(_ address: MEEmailAddress) -> String {
        address.addressString ?? address.rawString
    }

    /// A date, as the far end will read it.
    ///
    /// - Parameter date: the date, when Mail knows one.
    /// - Returns: the date in ISO 8601, or an empty string.
    static func timestamp(_ date: Date?) -> String {
        guard let date else {
            return ""
        }
        return ISO8601DateFormatter().string(from: date)
    }
}
