//
//  ConduitMailExtension.swift
//  ConduitMail
//
//  The principal class of the extension.
//
//  Mail has one extension point, `com.apple.email.extension`, and it asks
//  the principal class for the object that handles each capability the
//  extension declares in its Info.plist.  This one declares two:
//
//      MEComposeSessionHandler   ->  handler(for:)
//      MEMessageActionHandler    ->  handlerForMessageActions()
//
//  The other two capabilities of MailKit — the content blocker and the
//  message security handler — are not declared, so Mail never asks for
//  them.
//

import Foundation
import MailKit

@MainActor
final class ConduitMailExtension: NSObject, MEExtension {

    /// The handler of the compose window.
    private let compose = ConduitMailCompose()

    /// The handler of the messages Mail downloads.
    private let action = ConduitMailAction()

    // MARK: - MEExtension

    /// The handler of the message the user is writing.
    func handler(for session: MEComposeSession) -> any MEComposeSessionHandler {
        compose
    }

    /// The handler of the messages Mail downloads.
    func handlerForMessageActions() -> any MEMessageActionHandler {
        action
    }
}
