# The Thunderbird surface

Thunderbird's mail extensions are WebExtensions, so they run in the
browser: they cannot open a Unix socket.  The extension publishes the
message in front of the user to the near end over the loopback.

| the part | the file |
|---|---|
| the manifest Thunderbird reads | `manifest.json` |
| the background script | `background.js` |
| what a message is | `publish.js` |
| the declaration | `spec/surfaces.json` |
| the check | `check.mjs` |

## What it publishes

The message in front of the user: its subject, its sender, its date,
its folder and its id, under the schema
`dev.conduit.thunderbird.message/v1`.

## What was verified

**Verified on this machine.** `check.mjs` starts a real broker and a
real near end, connects to it over the loopback with the near end's
web client — the same module `background.js` imports — runs
`publish.js`, and the client at the far end receives the message,
opened.

**Not verified:** Thunderbird itself.  Thunderbird was never launched
here, the extension was never loaded by it, and `messenger.messageList`
was never called.  What was run is the publishing half and the loopback
path it needs.
