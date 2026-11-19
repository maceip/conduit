# The Apple Shortcuts surface

Shortcuts has no plugin format of its own: an app offers its actions to
Shortcuts with **App Intents**.  The surface is two Swift types:

* an `AppIntent` — the action itself, with the metadata the system needs;
* an `AppShortcutsProvider` — the phrases that make the action reachable
  from Shortcuts and from Siri.

The compiler reads that metadata out of the Swift and writes it into the
app, so **there is no manifest file**: there is nothing to register and
nothing to install.  The actions are in the app as soon as the app is
built.  (The declaration in `spec/surfaces.json` is Conduit's own
declaration of the surface — what the near end is told about it — and
not a manifest of the app.)

| the part | the file |
|---|---|
| the declaration | `spec/surfaces.json` |
| the action and the phrases | `ConduitShortcuts.swift` |
| the client of the near end | `NearEnd.swift` |

## What it publishes

The state the action knows when it runs: what the user asked to publish
(the label and the value), which app is publishing it, and when.  It goes
under the schema `dev.conduit.shortcuts.state/v1`.

The action publishes in plain JSON to the near end.  It holds no key, it
opens no socket to the broker, and it seals nothing: the near end does the
sealing, and the broker only ever sees the sealed form.

The publish is a frame of the tunnel, with a plaintext payload:
`{"t":"publish","s":"dev.conduit.mail.message/v1","c":"messages","p":{…}}`.
There is no second vocabulary on that socket.

The extension does not wait for the tunnel to agree its keys.  The near
end holds what is published until the two ends have met, and sends it
then; the waiting happens once, in the near end, and not in every
plugin in every language.  The near end says the tunnel is up with the
welcome frame, and complains with an error frame.

## Where the action runs

An App Intent runs **in the app that owns it**, not in Shortcuts: Shortcuts
asks the app to run the action.  So the socket of the near end has to be
one the app can open — the environment variable `CONDUIT_SOCKET` first,
then the shared defaults of the app group `group.dev.conduit.shortcuts`
under the key `nearEndSocket`, and then `/tmp/conduit-shortcuts.sock`.

## What the user has to do

1. **Put the two types in an app.**  The app needs the `AppIntents`
   framework and a deployment target of macOS 13 or iOS 16 or later.  The
   `AppShortcutsProvider` has to be in the app target itself, not in an
   extension, or Shortcuts will not see the phrases.
2. **Build it.**  `xcodebuild -scheme <the app> -destination 'platform=macOS' build`.
3. **Point the app at the near end.**  Set `CONDUIT_SOCKET` for the app,
   or write the path into the app group defaults.
4. **Use it.**  Open Shortcuts, search for the action, add it to a
   shortcut, and run it.  The action publishes the state, and the client at
   the far end receives it, opened.

## What was verified

**Nothing was compiled and nothing was run.**  This machine has no macOS,
no Xcode and no Swift toolchain, so `ConduitShortcuts.swift` and
`NearEnd.swift` have never been parsed by a compiler, the action has
never been built into an app, and Shortcuts has never run it.  What was
written was written by hand against Apple's published App Intents API
(`AppIntent`, `AppShortcutsProvider`, `AppShortcut`), and the protocol
against the contract in `docs/surface-contract.md` and the Node client in
`clients/native/near-end.js`.

To run it, on a Mac with Xcode:

* build an app target that contains the two files,
* run a near end with `spec/surfaces.json`,
* run the action from Shortcuts, and
* write a `check.mjs` in this directory that drives the *same* publishing
  code against a live broker and a live near end, the way
  `surfaces/vscode/check.mjs` does.  There is no such check here: it
  would have to run the Swift, and Swift cannot run here.
