# The Figma surface

A Figma plugin is short-lived and always user-invoked: "It's not
possible to build plugins that run in the background."  So it cannot
hold a socket, and it does not try — it publishes to the near end over
the loopback.

| the part | the file |
|---|---|
| the manifest Figma reads | `manifest.json` |
| the main thread | `code.js` |
| the iframe that holds the connection | `ui.html`, `ui.js` |
| what a selection is | `publish.js` |
| the declaration | `spec/surfaces.json` |
| the check | `check.mjs` |

## What it publishes

The nodes the user has selected: how many, and their ids, names and
types, under the schema `dev.conduit.figma.selection/v1`.

## What was verified

**Verified on this machine.** `check.mjs` starts a real broker and a
real near end, connects to it over the loopback with the near end's
web client — the same module the iframe uses — runs `publish.js`, and
the client at the far end receives the selection, opened.

**Not verified:** Figma itself.  Figma was never launched here, the
plugin was never installed or run in the editor, and the sandbox the
plugin runs in was never entered.  What was run is the publishing half
and the loopback path it needs.
