# architecture

the tunnel, its two ends, the broker in the middle. first written 2026-11-18.

## what this answers

> "Where does it go?  Where is the other end?"

the tunnel does not go to the app. the app sits behind the extension, and
the extension is the only thing that can see the app. the tunnel runs from
an extension to a broker; the broker pairs it with a client that asked for
it. the client never sees the app's process, memory or permissions; it sees
exactly what the user granted the extension + exactly what the extension
publishes under that grant.

```
   the app                    the broker                    the client
┌──────────────────┐      ┌─────────────────────┐     ┌──────────────────┐
│                  │      │                     │     │                  │
│  ┌────────────┐  │      │  registry           │     │  CLI / SDK       │
│  │  surface   │──┼──────┼─▶ scopes & grants   │◀┼────┼─▶ IDE / agent   │
│  │ (extension │  │ WSS  │  schema registry    │ │    │                  │
│  │  / plugin) │◀─┼──────┼─▶ typed relay       │     │                  │
│  └────────────┘  │      │  replay ring        │     │                  │
│                  │      │  (sees no payload)  │     │                  │
│  the app's own   │      └─────────────────────┘     └──────────────────┘
│  permissions,   │               ▲   ▲
│  the app's own  │               │   │  payload is sealed end to end
│  context        │               │   │
└──────────────────┘       the surface       the client
                           holds the app's   holds the client's
                           context           context
```

### two ends

surface = the extension or plugin inside the app. it runs with the app's
own permissions (in a browser it's an MV3 service worker with the host
permissions the user granted; in a desktop app it's a plugin). it's the
only component that can read app state and decides what to publish. for
runtimes that can't hold a socket or key (safari, short-lived per-event
plugins like MailKit, Shortcuts, or WordPress), an optional local bridge
(`crates/conduit-near-end`) can hold the socket and seal frames on its
behalf.

client = a CLI, an IDE integration, an agent harness, another extension.
it subscribes to a *surface* (a named channel with a schema and a scope
set) and receives the frames the surface publishes under it.

middle = the broker. deliberately dumb about content: it routes frames,
enforces scopes, validates the envelope, keeps a short replay ring, and
never sees a payload (payloads are sealed end to end between the two ends).
it's what makes the tunnel *typed*, *scoped* and *schemed* without trusting
either end.

### why there's a broker at all

the two ends can't meet directly:

* the extension is inside the app's sandbox and can't listen on a port;
* the client is somewhere else entirely;
* the user has to be able to revoke one surface without killing the others,
  and audit what crossed it;
* either end can be absent (app closed, client gone) and still has to catch
  up when it returns.

the broker is the only party that sees both sides' metadata => it's the only
party that can hold the grant, so it has to be the one that can do the
least.

---

## typed

every frame carries a type from a registry of frame types + a schema id for
its payload.

```
{ "t": "event", "s": "dev.conduit.mail.message/v1", "c": "mail.messages",
  "i": 41, "seq": 9182, "p": "<sealed>" }
```

* `t`: frame type. registered, versioned, closed; an unknown type is
  refused, never ignored. registry is `spec/frames.json`.
* `s`: payload schema, a reverse-dns id with a version
  (`dev.conduit.mail.message/v1`); same shape MCP uses for its extensions
  (`io.modelcontextprotocol/ui`).
* `c`: the channel the frame belongs to.
* `i`: call id, for frames that answer a call.
* `seq`: the tunnel's monotonic sequence number, for resume.
* `p`: the payload, sealed.

the broker validates the envelope against the frame schema. the receiver
validates the payload against the schema the frame names, since the broker
can't read it.

## scoped

a surface = a named capability the extension offers, with a schema set and a
scope set. the extension declares what it can do; the user grants a subset;
the broker holds the grant; the client subscribes with the scopes it needs.

```
surface dev.conduit.mail/compose/v1
  schemas   dev.conduit.mail.draft/v1
  scopes    mail.compose.read  mail.compose.write
```

3 rules; that's the entire broker-side security model:

1. a subscription naming a scope the grant doesn't cover is refused, with
   the scope named in the error.
2. a frame naming a scope outside the grant is dropped and counted. the
   count is reported to the extension, which can show it.
3. grants are per surface, per extension instance, revocable. revoking one
   closes the subscriptions it covered and leaves the rest alone.

scopes are namespaced by the surface that defines them (`mail.compose.write`
belongs to `dev.conduit.mail/compose/v1`) => another surface can't forge
one, and a client can't widen one by asking twice.

## schemed

2 schemes; the word covers both.

address scheme: a tunnel is addressed by a URI:

```
conduit://mail/compose@v1
conduit://chrome/tabs@v1
```

`conduit:` says what kind of thing is addressed; the authority names the
surface; the path names the channel; the version pins the schema set. a
client can hold a `conduit:` URI and nothing else, and a surface can be
published under its own scheme (e.g. `mailto:` for a mail surface) with the
conduit scheme as fallback. this is MCP's `ui://` idea, with the authority
doing the job MCP's extension id does.

encryption scheme: the payload is sealed with a scheme the grant names:

```
"crypto": "p256-aes256gcm/v1"
```

the two ends agree a key at subscription time; the broker only sees the
envelope. default is the scheme above; a surface can require another
(`none` for a surface the user explicitly declared local, i.e. a
loopback-only tunnel with no broker at all; see "loopback" below).

---

## transports per platform

this is where the research pays off.

### chromium: surface runs in a service worker

* tunnel is a WebSocket to the broker.
* ping every 20s. from Chrome 116, WebSocket traffic resets the 30s
  service-worker idle timer; an idle socket doesn't, and neither does a
  retry loop against a dead broker. 20s sits inside the 30s window with
  slack for a slow frame.
* resume by sequence number. last seen seq is kept in
  `chrome.storage.session` and sent on reconnect; the broker replays from
  its ring.
* reconnect with exponential backoff + jitter, capped at 30 s.
* the client of a chromium surface can itself be a chromium extension; then
  both ends meet in the same broker and never touch the network.

### safari: optional local bridge (native host)

* the extension's background is a background page (`background.scripts`),
  because an MV3 service worker never started on Safari 26.5.2, and Safari
  unloads idle background contexts after ~30–45 s whether or not a
  `sendNativeMessage` promise is in flight.
* the background page talks to a native host over
  `browser.runtime.connectNative`; Safari ignores the application id and
  always talks to the containing app's native extension.
* the native host holds the WebSocket. WebSocket connections to `127.0.0.1`
  are broken in Safari => no loopback fallback; and the native host
  survives the background page dying, which is what a tunnel needs.
* the native host is the optional local bridge in `--native` mode
  (`crates/conduit-near-end`), since native
  messaging runs any executable and the rest of the project is Rust.
* Safari native messaging has a measured 150-active-request guard and needs
  ~40 ms spacing between calls => the host keeps 1 request in flight and
  paces the rest; tunnel frames are batched instead of sent one at a time.

### same manifest, both browsers

`clients/chromium-mv3/manifest.json` and `clients/safari-mv3/manifest.json`
declare both `background.scripts` and `background.service_worker`; chromium
takes the worker, safari takes the scripts. script source is shared; the
build emits the right one per target.

---

## loopback: tunnel with no broker

not every tunnel needs a middle. a surface can declare itself local: the
extension connects to a host on the machine, the client on the same machine
connects to the same host, and the broker is replaced by a tiny local
rendezvous that never leaves the machine.

why: a user who doesn't want their mail leaving the laptop shouldn't have
to trust a hosted broker with a grant on it. the local rendezvous enforces
the same scopes, schemas and envelope; the only difference is it's on the
machine and throwaway.

safari can't use it (localhost WebSockets are broken there) => on safari the
local rendezvous is reached through the native host over native messaging,
the same path the hosted tunnel takes.

---

## the broker, concretely

| concern | what it does |
|---|---|
| registry | surfaces + their schemas, scopes, schemes |
| grants | what the user granted, per surface, per extension instance; revocable |
| relay | typed frames end to end, with the scope check |
| replay | short ring per tunnel, for resume after a gap |
| auth | extension proves who it is; client proves who it is |
| payload | never sees one |

extension auth = device code: it opens a page in the user's browser, the
user approves the surface there, the extension gets a long-lived secret. the
extension can be signed by the user instead; that's stronger, and it's what
kind-3 surfaces (OS app extensions) can do: an app extension is already
signed + sandboxed by the OS and can present its identity to the broker via
the OS's own keychain.

client auth = a token it holds; the broker maps the token to the set of
surfaces it may subscribe to.

---

## client interface

the far end is a library first, CLI second, since the clients that matter
are agents and IDEs.

```rust
let mut surface = client.subscribe("conduit://mail/compose@v1").await?;
surface.on_event(|msg: mail::Message| { ... }).await?;
```

the library is typed over the *schema*: a `dev.conduit.mail.message/v1`
payload arrives as a Rust struct generated from the JSON Schema; a payload
that doesn't match its schema is a protocol error, never a string that
happens to be JSON.

---

## decided vs open

decided: two ends + middle; frame envelope; scope model; scheme model;
per-platform transports; loopback mode.

open: can the broker ever see a payload (today: no); can a grant be
delegated (today: no); how a surface's schema set is published (today: the
extension declares it, the broker records it); client auth in the first
release (today: a token, with a device-code flow on the extension side).

not designed yet: the adapters, i.e. the per-app code that knows how to read
a Mail message or a browser tab. they get written per app family, and
they're where the research in `docs/research/extension-surfaces.md` becomes
code.
