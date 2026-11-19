# The VS Code surface

VS Code runs an extension in its own extension host process, and that
process dies with the window.  So the extension does not hold a socket
to the broker and never sees a key: it publishes what the editor has
open to the near end, which does the sealing.

| the part | the file |
|---|---|
| the manifest VS Code loads | `package.json` |
| the extension | `extension.js` |
| what a tab is | `publish.js` |
| the declaration | `spec/surfaces.json` |
| the check | `check.mjs` |

## What it publishes

The tab in front of the user: its title, its language, its path and its
URL, under the schema `dev.conduit.vscode.tab/v1`.

## What was verified

**Verified on this machine.** `check.mjs` starts a real broker and a
real near end, runs `publish.js` — the same module `extension.js`
imports — and the client at the far end receives the tab, opened.

**Not verified:** the extension inside a running VS Code. The VS Code
API is not present on this machine, so `extension.js` itself has never
been loaded by an editor. What was run is the publishing half of it.
