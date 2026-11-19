# conduit

![conduit](docs/social-preview-rounded.png)

most apps give third parties no way in. apple ships 400+ app extensions and most users never see the list.

conduit is a protocol + reference impl that lets a client (cli, ide, agent) reach state inside an app over a tunnel the user can see and revoke. the client never touches the app's process, memory or permissions; it gets what the user granted and nothing else.

![The extension, the broker and the client](docs/diagrams/topology.svg)

more => [`docs/architecture.md`](docs/architecture.md), [`docs/protocol.md`](docs/protocol.md)

## the protocol

pair once => the extension asks the broker for a code, the user approves it on the broker's own page, the extension gets a 256-bit secret bound to exactly those scopes.

![Pairing an extension](docs/diagrams/pairing.svg)

then both ends open a websocket to the broker and say hello. hello carries a public key + the last `seq` seen; on reconnect the broker replays what you missed from a ring of the last 256 frames. payloads stay sealed end to end and the broker refuses anything outside the grant.

![A session](docs/diagrams/session.svg)

## typed, scoped, schemed

- typed => every frame names a versioned reverse-dns schema, e.g. `dev.conduit.mail.message/v1`. unknown frame type: refused.
- scoped => a surface offers scopes (`mail.compose.read`), the user grants a subset, the broker enforces it. out of grant: refused, with the missing scope named.
- schemed => address is `conduit://<surface>/<channel>@v1`. seal is `p256-aes256gcm/v1` (ecdh p-256, hkdf-sha256, aes-256-gcm); the broker never opens it.

rust in `crates/`, chromium + safari extensions in `clients/`, json schemas in `spec/`, app survey in `docs/research/`.

## the surfaces

a surface is a module: an id, a channel, the schemas it may carry, the scopes it asks for, and a read. the tunnel machinery carries any surface and knows nothing about what it carries, so a new surface is a new module and not a new tunnel.

| surface | module | what it publishes |
|---|---|---|
| the browser tab | `clients/shared/surface-browser.js` | the url + title of the tab in front of the user |
| the browser windows | `clients/shared/surface-windows.js` | the windows the browser has open, and how many tabs each holds |

a surface never publishes a stand-in: when the read says there is nothing, nothing is published.

outside a browser, a surface is three parts: the app, the plugin inside it, and the **near end** — a process that holds the socket and does the sealing, because a plugin cannot be trusted with either. fifteen of them are here:

| surface | the app | what it publishes |
|---|---|---|
| [`apple-mail`](surfaces/apple-mail) | apple mail | the message being composed, the message selected |
| [`apple-shortcuts`](surfaces/apple-shortcuts) | apple shortcuts | what the owning app says about the state |
| [`antigravity`](surfaces/antigravity) | google antigravity | the state of the surface, as an mcp tool |
| [`discord`](surfaces/discord) | discord | the message of the channel |
| [`figma`](surfaces/figma) | figma | the current selection |
| [`home-assistant`](surfaces/home-assistant) | home assistant | the state of the entities the user listed |
| [`mcp`](surfaces/mcp) | any mcp host | the state of the surface, as an mcp tool |
| [`neovim`](surfaces/neovim) | neovim | the buffer in front of the user |
| [`obsidian`](surfaces/obsidian) | obsidian | the note in front of the user |
| [`safari`](surfaces/safari) | safari | the tab in front of the user |
| [`slack`](surfaces/slack) | slack | the message of the channel |
| [`stream-deck`](surfaces/stream-deck) | elgato stream deck | the key that was pressed |
| [`thunderbird`](surfaces/thunderbird) | thunderbird | the message in front of the user |
| [`vscode`](surfaces/vscode) | vs code | the tab in front of the user |
| [`wordpress`](surfaces/wordpress) | wordpress | the site, and each post |

what was verified and what was not is in [`docs/surfaces.md`](docs/surfaces.md). in short: eight of them are checked end to end here — a real broker, a real near end, the plugin's own code — and the ones whose runtime is not on this machine (apple mail, apple shortcuts, wordpress, neovim) are written and **not run**, in those words.

whatsapp has no extension surface at all: everything third-party is server-side, so there is no whatsapp surface here, and the reason is in [`docs/research/app-catalogue.md`](docs/research/app-catalogue.md).

## run it

```sh
cargo test --workspace
node --test clients/shared/*.test.js # the surfaces, and the sealing
node scripts/browser-test.mjs       # sealing, in real Chromium
scripts/e2e/run.sh                  # broker + extension in chromium + rust client, end to end
node scripts/surfaces/run.mjs       # every surface, against a real broker
cargo run -p conduit-broker         # ws://127.0.0.1:8787/v1/tunnel
cargo run -p conduit-near-end       # the near end: the socket, and the sealing
node scripts/build-extension.mjs    # => target/extension/{chromium,safari}-mv3, load unpacked
```

## status

shipped + tested: protocol, broker, client lib, the near end, both extensions, two surfaces in the browser extension, and fifteen surfaces outside it. rust and browser sealing match byte for byte on a shared test vector.

verified in chromium: [`scripts/e2e`](scripts/e2e/README.md) loads the extension, pairs it on the broker's page, and a rust client opens the sealed tab it publishes; runs in ci.

verified outside a browser: eight surfaces are checked here against a real broker and a real near end, with the plugin's own code; the ledger is in [`docs/surfaces.md`](docs/surfaces.md).

not verified: safari itself, and the four surfaces whose runtime is not on this machine — apple mail, apple shortcuts, wordpress and neovim. their code is written and was never run.

## why research first

chromium and safari decided most of this. chromium kills an idle service worker after 30s => the near end pings every 20s and resumes by `seq`. safari unloads background pages about as fast and its websocket to `127.0.0.1` is broken => a native host holds the socket. sources: [`extension-surfaces.md`](docs/research/extension-surfaces.md)

## license

MIT or Apache-2.0, your pick. [`LICENSE`](LICENSE), [`LICENSE-APACHE`](LICENSE-APACHE)
