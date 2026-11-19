# The Stream Deck surface

An Elgato Stream Deck plugin.  The driver loads the plugin and speaks
to it over a WebSocket on the loopback; the plugin reads what the keys
say and publishes it
([SDK](https://docs.elgato.com/stream-deck/sdk/)).

The plugin holds no key and opens no socket to the broker.  It
publishes what the key says, in plain JSON, to the near end, which does
the sealing.

| the part | the file |
|---|---|
| the manifest of the plugin | `manifest.json` |
| the plugin | `index.js` |
| what a pressed key says | `publish.js` |
| the declaration | `surface.json` |
| the property inspector | `property-inspector.html` |
| the icons | `imgs/` |
| the check | `check.mjs` |

`package.json` is here only to tell Node that the files in this
directory are CommonJS, and to name the SDK the plugin uses
(`@elgato/streamdeck`).

## What it publishes

The key that was pressed, under the schema
`dev.conduit.streamdeck.key/v1`: the row and the column of the key in
the grid, the device it is on, the state of the key, and that it went
down.

The Conduit scope of the surface is `streamdeck.keys.read`.

## The Stream Deck side of it

`index.js` registers the action of the manifest
(`dev.conduit.streamdeck.key`) with the SDK — the SDK's call is
`StreamDeck.actions.registerAction(action)` — and its `keyDown` handler
publishes the press.  The SDK is published as an ES module, so the
plugin loads it with an import.

The manifest is the action manifest of the SDK: `Actions`, with a
`Name`, `UUID`, `Icon`, `Icons`, `Tooltip`, `States`, `SupportedDevices`,
`SupportedIn`, `PropertyInspectorPath` and the `Encoder` settings, and
the plugin's entry point in `CodePath`.  It is written in the shape of
SDK version 1, which is the shape that has `SupportedDevices` and
`SupportedIn`; the current SDK (version 2) replaced those two with
`Controllers`, and a manifest for it would say `"SDKVersion": 2`.  The
manifest was never checked by the Stream Deck application.

The images in `imgs/` are placeholders: white rings on a transparent
ground, which is what the SDK asks of an icon.

## What was verified

**Verified on this machine.** `check.mjs` starts a real broker and a
real near end, runs `publish.js` — the same module `index.js` imports
— and the client at the far end receives the key, opened.  The frame
that crossed the broker was not the key in the clear.

**Verified on this machine, in part.** The shape of the action in
`index.js` was checked against the real SDK (`@elgato/streamdeck`
1.4.1) outside the Stream Deck application: the SDK reads
`manifest.json` from the working directory, found the UUID of the
action, and accepted the registration.  That was a probe of the shape
of the plugin, not the plugin itself.

**Not verified: the Stream Deck side.** There is no Stream Deck device
and no Stream Deck application on this machine, so `index.js` has never
run as a plugin: the connection to the driver and the `keyDown` handler
are **not run**, and only the publishing half of the surface is verified
end to end.
