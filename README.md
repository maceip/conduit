# Conduit

![Conduit](docs/social-preview.jpg)

**A typed, scoped, schemed tunnel from inside an app to a client.**

Most apps have no seam a third party can use.  Some hide theirs — Apple
runs over four hundred app extensions that most users will never see
listed — and some have none at all.  Conduit is a **protocol and a
reference implementation** for the seam that does not exist yet: a way for
a client to reach the state inside an app, over a tunnel that is typed,
scoped and schemed, and that the user can see and revoke.

```
   the app                    the broker                    the client
┌──────────────────┐      ┌─────────────────────┐     ┌──────────────────┐
│  ┌────────────┐  │      │  registry           │     │  CLI / SDK       │
│  │ extension  │──┼──────┼─▶ scopes & grants   │◀┼────┼─▶ IDE / agent   │
│  │ (near end) │  │ WSS  │  schema registry    │     │  (far end)      │
│  └────────────┘  │◀─────┼─▶ typed relay       │     └──────────────────┘
└──────────────────┘      │  replay ring        │
                           │  (sees no payload)  │
                           └─────────────────────┘
```

**Where does it go?  To the broker.**  **Where is the other end?  A
client** — a CLI, an IDE, an agent — that never sees the app's process,
its memory or its permissions.  It sees exactly what the user granted to
the extension.  The full argument is in [`docs/architecture.md`](docs/architecture.md).

## What is in here

| | |
|---|---|
| [`docs/research/app-catalogue.md`](docs/research/app-catalogue.md) | the catalogue: one row per app, the surface it ships, and whether the user can see it |
| [`docs/research/extension-surfaces.md`](docs/research/extension-surfaces.md) | the survey by kind: web extensions, host plugins, OS app extensions, secret APIs, agent protocols, marketplaces |
| [`docs/architecture.md`](docs/architecture.md) | the topology, the two ends, and the broker |
| [`docs/protocol.md`](docs/protocol.md) | the frames, the scopes, the schemes, the errors |
| [`spec/`](spec) | the normative JSON Schemas and the frame registry |
| `crates/conduit-protocol` | the frames, the scopes, the grants, the replay ring |
| `crates/conduit-broker` | the relay: the registry, the grants, the scope check, the ring |
| `crates/conduit-client` | the far end: a typed client over a `conduit://` address |
| `crates/conduit-native-host` | the Safari near end: a native host that holds the socket |
| `clients/shared/sealing.js` | the sealing, in the browser: the same scheme as the Rust |
| `clients/shared/near-end.js` | the tunnel machinery, for any surface: the plug a surface drops into |
| `clients/shared/surface-browser.js` | the first surface: the tab in front of the user |
| `clients/shared/surface-windows.js` | the second surface: the windows the browser has open |
| `clients/chromium-mv3`, `clients/safari-mv3` | the extension clients |

## The three adjectives

**Typed.**  Every frame names its type and the schema of its payload, and
the schema is a reverse-DNS identifier with a version — `dev.conduit.mail.message/v1`,
the shape MCP uses for its extensions.  An unknown frame type is refused,
not ignored.

**Scoped.**  A surface offers scopes (`mail.compose.read`,
`mail.compose.write`); the *user* grants a subset; the broker holds the
grant; a subscription outside the grant is refused **with the scope
named**, and a frame outside the grant is dropped and counted.

**Schemed.**  Two schemes.  The address: `conduit://<surface>/<channel>@v1`,
so a client may hold a URI and nothing else.  The encryption: the payload
is sealed between the two ends, and the broker never opens it.

## The surfaces

A surface is a module — an identifier, a channel, a schema, a scope, and
a read — and `clients/shared/near-end.js` carries it.  The tunnel
machinery knows nothing about what it carries, and a read that says
there is nothing publishes nothing.

| surface | channel | schema | scope | reads |
|---|---|---|---|---|
| `dev.conduit.browser.tabs/v1` | `tabs` | `dev.conduit.browser.tab/v1` | `browser.tabs.read` | the tab in front of the user |
| `dev.conduit.browser.windows/v1` | `windows` | `dev.conduit.browser.window/v1` | `browser.windows.read` | the windows the browser has open |

The next surfaces are the ones the research names: a Mail adapter behind
`dev.conduit.mail.compose/v1`, and a desktop-app adapter.  They are
written per app family — the near end is not.

## Running it

```sh
cargo test --workspace              # the protocol, the broker, the client
node --test clients/shared/*.test.js # the surfaces, in node
node scripts/browser-test.mjs       # the sealing, in a real browser
node scripts/assemble-clients.mjs   # copy the shared near end into each client
node scripts/extension-e2e.mjs      # the extension and a far end, end to end
cargo run -p conduit-broker         # ws://127.0.0.1:8787/v1/tunnel
```

The broker's bootstrap carries the browser tabs surface the shipped
extension offers (`dev.conduit.browser.tabs/v1`) and the mail shape the
research names (`dev.conduit.mail.compose/v1`), each with a grant and
one shared token.

## The state of it, honestly

**Built and tested:** the protocol and its schemas, the broker's registry,
grants, scope enforcement and replay ring, the device flow, the client
library, the native host, the two extension clients, and the first
surface — the browser tab.  The broker and the client library are tested
against each other over real sockets, with the payload sealed end to end.

**The extension, in a real browser.**  `scripts/extension-e2e.mjs` loads
the Chromium client into a real Chromium, opens the tunnel to a local
broker through the extension's own toggle, and shows the Rust far end
opening a sealed frame carrying the real tab.  Three defects were found
by that run and are fixed: the extension did not present its secret on
the connection, it sealed frames under the peer's nonce prefix instead
of its own, and the shared near end was never assembled into the
extension directory.  The run covers Chromium; the Safari client and its
native host are tested at the socket level and have not been loaded into
Safari.

**The sealing works.**  The scheme is `p256-aes256gcm/v1`: ECDH over
P-256, HKDF-SHA256 into two directional keys, and AES-256-GCM under a
nonce that carries the sequence number.  The browser and the Rust are
checked against the same vector, and the browser half runs in a real
Chromium, because Node's WebCrypto cannot import the key the vector
needs.  The ciphertext is the same byte for byte on both sides.

**Not built:** the other surfaces.  There is a browser tab, and no Mail
adapter and no desktop-app adapter yet — the pieces that know how to read
a message inside an app.  They are the part that has to be written per
app family, and they are the reason the research came first.

**Not verified in Safari:** the native-host path.  Chromium's near end
holds the socket itself and is verified end to end; Safari's near end
hands the socket to the native host, which is tested over real sockets
but has not been driven from Safari itself.

**Not decided:** whether the broker may ever see a payload (today: no),
whether a grant may be delegated (today: no), and how a surface's schema
set is published (today: the extension declares it).

## Why the research came first

Because the platforms decide the design, not the other way round:

* a Chromium service worker is killed after 30 seconds of silence, so the
  near end pings every 20 seconds and resumes by sequence number;
* a Safari background page is unloaded after about 30 seconds, and its
  WebSocket to `127.0.0.1` is broken, so on Safari the near end is a
  **native host** that holds the socket;
* MCP's extensions framework already has the two ideas worth copying — the
  reverse-DNS identifier and the `ui://` scheme — and the one worth
  refusing: a request/response protocol with an open transaction does not
  survive a worker that dies every thirty seconds.

The sources are at the end of
[`docs/research/extension-surfaces.md`](docs/research/extension-surfaces.md),
and the claims that were not checked are marked as such.

## Licence

MIT or Apache-2.0, at your option.  See [`LICENSE-MIT`](LICENSE-MIT) and
[`LICENSE-APACHE`](LICENSE-APACHE).
