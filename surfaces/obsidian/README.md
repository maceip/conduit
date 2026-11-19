# The Obsidian surface

Obsidian loads a plugin as a CommonJS module: a `manifest.json` and a
`main.js` whose default export is a class with an `onload`.  The plugin
publishes the note in front of the user to the near end, which seals it.

| the part | the file |
|---|---|
| the manifest Obsidian reads | `manifest.json` |
| the plugin | `main.js` |
| what a note is | `publish.js` |
| the declaration | `surface.json` |
| the check | `check.mjs` |

## What it publishes

The note in front of the user: its path, its basename, its last
heading, its tags and an excerpt, under the schema
`dev.conduit.obsidian.note/v1`.

## What was verified

**Verified on this machine.** `check.mjs` starts a real broker and a
real near end, runs `publish.js` — the same module `main.js` imports —
and the client at the far end receives the note, opened.

**Not verified:** Obsidian itself.  Obsidian was never launched here
and the plugin was never loaded by a vault, so `onload`, the workspace
events and the metadata cache were never exercised.  What was run is
the publishing half.
