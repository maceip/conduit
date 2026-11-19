//
//  ConduitShortcuts.swift
//  ConduitShortcuts
//
//  The surface of an app whose actions Shortcuts can run.
//
//  The surface is App Intents: an `AppIntent` type, and an
//  `AppShortcutsProvider` that publishes the app's actions to Shortcuts.
//  The compiler reads the metadata out of the Swift and writes it into the
//  app, so there is no manifest file here and nothing to register: the
//  actions are in the app as soon as the app is built.
//
//  What the intent publishes is plaintext to the near end.  It holds no
//  key and seals nothing: the near end does the sealing, and the broker
//  only ever sees the sealed form.
//

import AppIntents
import Foundation

/// The action: publish the current state of the app.
///
/// The state is what the action knows when it runs: what the user asked
/// to publish, which app is publishing it, and when.
@available(macOS 13.0, iOS 16.0, *)
struct PublishConduitStateIntent: AppIntent {

    /// The schema of a state.
    static let schema = "dev.conduit.shortcuts.state/v1"

    /// What the action is called.
    static var title: LocalizedStringResource = "Publish the current state"

    /// What the action does.
    static var description = IntentDescription(
        "Publishes the current state of this app to a Conduit client.")

    /// The action runs in the background: it does not need the app open.
    static var openAppWhenRun = false

    /// What is being published.
    @Parameter(title: "Label", default: "state")
    var label: String

    /// The state itself.
    @Parameter(title: "Value")
    var value: String?

    /// Publish the state.
    func perform() async throws -> some IntentResult & ProvidesDialog {
        let payload: [String: Any] = [
            "label": label,
            "value": value ?? "",
            "application": PublishConduitStateIntent.applicationName(),
            "applicationIdentifier": Bundle.main.bundleIdentifier ?? "",
            "publishedAt": ISO8601DateFormatter().string(from: Date()),
        ]

        // The publish is a socket write and a wait for the tunnel to agree
        // its keys, so it is done here rather than on the main actor.
        let nearEnd = NearEnd()
        do {
            try nearEnd.publish(schema: PublishConduitStateIntent.schema, payload: payload)
        } catch {
            return .result(dialog: IntentDialog(stringLiteral:
                "Conduit could not publish the state: \(error.localizedDescription)"))
        }
        return .result(dialog: IntentDialog(stringLiteral: "Conduit published \(label)."))
    }

    /// The name of the app that owns the action.
    static func applicationName() -> String {
        let info = Bundle.main.infoDictionary
        if let name = info?["CFBundleDisplayName"] as? String, !name.isEmpty {
            return name
        }
        if let name = info?["CFBundleName"] as? String, !name.isEmpty {
            return name
        }
        return Bundle.main.bundleIdentifier ?? "the app"
    }
}

/// The actions of the app, as Shortcuts sees them.
///
/// Every phrase has to say the name of the app, so that Siri knows which
/// app to open; the applicationName token is how that name is written.
@available(macOS 13.0, iOS 16.0, *)
struct ConduitShortcuts: AppShortcutsProvider {

    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: PublishConduitStateIntent(),
            phrases: [
                "Publish the current state with \(.applicationName)",
                "Publish my state in \(.applicationName)",
                "Share the current state with \(.applicationName)",
            ],
            shortTitle: "Publish the current state",
            systemImageName: "antenna.radiowaves.left.and.right"
        )
    }
}
