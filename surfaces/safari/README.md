# The Safari surface

Safari's background page is unloaded after about thirty seconds of
silence, and a loopback socket does not survive that.  So on Safari the
socket lives in a native process — the **near end** — and the extension
talks to it over the browser's native messaging protocol.

| the part | the file |
|---|---|
| the extension Safari loads | `../../clients/safari-mv3/` |
| the native messaging host | `../../crates/conduit-near-end/` in its `--native` mode |
| what a tab is | `publish.js` |
| the declaration | `spec/surfaces.json` |
| the check | `check.mjs` |

The near end holds the socket, the keys and the sealing.  The extension
publishes what the user has in front of them, in plain JSON, and the
near end seals it.  Nothing the extension publishes is readable by the
broker.

## What was verified

**Verified on this machine.** `check.mjs` starts a real broker and the
near end in its native mode, speaks the native messaging protocol to it
over a pipe exactly as Safari does, and the client at the far end
receives the tab, opened and sealed on the way.

**Not verified:** Safari itself.  Safari was never launched here, the
extension was never loaded by Safari, and the native messaging host was
never registered with Safari — that registration is a plist in
`~/Library/Application Support/com.apple.Safari/NativeMessagingHosts`
and it needs a macOS to install.  The extension's own JavaScript is
shared with the Chromium extension, which *is* exercised by the browser
test (`scripts/browser-test.mjs`).
