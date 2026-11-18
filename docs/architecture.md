# The architecture

**The tunnel, its two ends, and the broker in the middle.**  First written
2026-11-18.

## The question this answers

> "Where does it go?  Where is the other end?"

**The tunnel does not go to the app.**  The app is on the far side of the
extension, and the extension is the only thing that can see the app.  The
tunnel goes from an **extension** to a **broker**, and the broker pairs it
with a **client** that asked for it.  The client never sees the app's
process, its memory, or its permissions; it sees exactly what the user
granted to the extension, and exactly what the extension publishes under
that grant.

```
   the app                    the broker                    the client
┌──────────────────┐      ┌─────────────────────┐     ┌──────────────────┐
│                  │      │                     │     │                  │
│  ┌────────────┐  │      │  registry           │     │  CLI / SDK       │
│  │ extension  │──┼──────┼─▶ scopes & grants   │◀┼────┼─▶ IDE / agent   │
│  │ (the near  │  │ WSS  │  schema registry    │ │    │  (the far end)  │
│  │  end)      │◀─┼──────┼─▶ typed relay       │     │                  │
│  └────────────┘  │      │  replay ring        │     │                  │
│                  │      │  (sees no payload)  │     │                  │
│  the app's own   │      └─────────────────────┘     └──────────────────┘
│  permissions,   │               ▲   ▲
│  the app's own  │               │   │  payload is sealed end to end
│  context        │               │   │
└──────────────────┘      the near end      the far end
                          holds the app's   holds the client's
                          context           context
```

### The two ends

**The near end** is the extension.  It runs inside the app's own sandbox,
with the app's own permissions — in Mail it is a MailKit extension with
access to the message being composed; in a browser it is a service worker
with the host permissions the user granted it.  It is the only component
that can read the app's state, and it is the only component that decides
what to publish.

**The far end** is the client: a command-line tool, an IDE integration, an
agent harness, another extension.  It subscribes to a *surface* — a named
channel with a schema and a scope set — and it receives frames that the
extension publishes under that surface.

**The middle** is the broker.  It is deliberately dumb about content: it
routes frames, enforces scopes, validates the envelope, keeps a short
replay ring, and it **never sees a payload** (payloads are sealed end to
end between the two ends).  It is the component that makes the tunnel
*typed*, *scoped* and *schemed* possible without trusting either end.

### Why the broker is in the middle at all

Because the two ends cannot meet directly:

* the extension is inside the app's sandbox and cannot listen on a port;
* the client is somewhere else entirely;
* the user has to be able to **revoke** one surface without killing the
  others, and **audit** what crossed it;
* the two ends must be able to be **absent** — the app closed, the client
  gone — and still catch up when one returns.

The broker is the only party that sees both sides' metadata, so it is the
only party that can hold the grant, and it is the party that must be
smallest in what it can do.

---

## Typed

Every frame carries a **type** from a registry of frame types, and every
frame carries a **schema id** for its payload.

```
{ "t": "event", "s": "dev.conduit.mail.message/v1", "c": "mail.messages",
  "i": 41, "seq": 9182, "p": "<sealed>" }
```

* `t` — the frame type.  Registered, versioned, and closed: an unknown
  type is refused, not ignored.  The registry is `spec/frames.json`.
* `s` — the schema of the payload, as a **reverse-DNS id with a version**
  (`dev.conduit.mail.message/v1`), the same shape MCP uses for its
  extensions (`io.modelcontextprotocol/ui`).
* `c` — the **channel** the frame belongs to.
* `i` — the call id, for frames that answer a call.
* `seq` — the monotonic sequence number of the tunnel, for resume.
* `p` — the payload, sealed.

The envelope is validated by the broker against the frame schema.  The
payload is validated by the **receiver**, against the schema the frame
names, because the broker cannot read it.

## Scoped

A **surface** is a named capability the extension offers, with a schema set
and a scope set.  The extension declares what it can do; the **user** grants
a subset; the broker holds the grant; the client subscribes with the
scopes it needs.

```
surface dev.conduit.mail/compose/v1
  schemas   dev.conduit.mail.draft/v1
  scopes    mail.compose.read  mail.compose.write
```

Three rules, and they are the whole of the security model on the broker
side:

1. **A subscription that names a scope the grant does not cover is
   refused** — with the scope named in the error.
2. **A frame that names a scope outside the grant is dropped**, and
   counted.  The count is reported to the extension, which can show it.
3. **The grant is per surface, per extension instance, and revocable** —
   revoking a grant closes the subscriptions it covered and does not touch
   the others.

Scopes are namespaced by the surface that defines them (`mail.compose.write`
belongs to `dev.conduit.mail/compose/v1`), so a scope cannot be forged by
another surface, and a client cannot widen one by asking for it twice.

## Schemed

Two schemes, and the word means both.

**The address scheme.**  A tunnel is addressed by a URI:

```
conduit://mail/compose@v1
conduit://chrome/tabs@v1
```

The scheme `conduit:` says what kind of thing is being addressed; the
authority names the surface; the path names the channel; the version
pins the schema set.  A client may hold a `conduit:` URI and nothing
else, and a surface may be published under a scheme of its own
(`mailto:` for a mail surface, for one) with the conduit scheme as the
fallback.  This is MCP's `ui://` idea, with the authority doing the work
that MCP's extension id does.

**The encryption scheme.**  The payload is sealed with a scheme the grant
names:

```
"crypto": "p256-aes256gcm/v1"
```

The two ends agree a key at subscription time; the broker sees only the
envelope.  The default is the scheme above; a surface may require another
(`none` for a surface the user has explicitly declared local — a
loopback-only tunnel with no broker at all, which §"Loopback" below
describes).

---

## The transports, per platform

This is where the research earns its place.

### Chromium — the near end is a service worker

* The tunnel is a **WebSocket** to the broker.
* **A ping every 20 seconds.**  From Chrome 116, WebSocket traffic resets
  the 30-second service-worker idle timer; an idle socket does not, and
  neither does a retry loop against a dead broker.  Twenty seconds is
  inside the thirty-second window with slack for a slow frame.
* **Resume by sequence number.**  The last sequence number seen is kept in
  `chrome.storage.session`, and sent on reconnect; the broker replays
  from its ring.
* **Reconnect with exponential backoff and jitter**, capped at 30 s.
* The client of a Chromium surface may itself be a Chromium extension — in
  which case the two ends meet in the same broker and never touch the
  network.

### Safari — the near end is a native process

* The extension's background is a **background page**
  (`background.scripts`), because an MV3 service worker never started on
  Safari 26.5.2, and because Safari unloads idle background contexts after
  ~30–45 s whether or not a `sendNativeMessage` promise is in flight.
* The background page talks to a **native host** over
  `browser.runtime.connectNative` — Safari ignores the application id and
  always talks to the containing app's native extension.
* **The native host holds the WebSocket.**  WebSocket connections to
  `127.0.0.1` are broken in Safari, so a loopback fallback is not
  available; and the native host survives the background page's death,
  which is exactly what a tunnel needs.
* The native host is a **Rust binary** (`clients/native-host`), because
  native messaging will run any executable and the rest of this project is
  Rust.
* Safari's native messaging has a measured 150-active-request guard and
  needs ~40 ms spacing between calls; the host therefore keeps **one**
  request in flight and paces the rest, and the tunnel's own frames are
  batched rather than sent one at a time.

### The same manifest, both browsers

`clients/chromium-mv3/manifest.json` and `clients/safari-mv3/manifest.json`
declare **both** `background.scripts` and `background.service_worker`;
Chromium takes the worker, Safari takes the scripts.  The script source is
shared; the build emits the right one per target.

---

## Loopback: the tunnel with no broker

Not every tunnel needs a middle.  A surface can declare itself **local**:
the extension connects to a host on the machine, and the client on the
same machine connects to the same host, and the broker is replaced by a
tiny local rendezvous that never leaves the machine.

This exists because a user who does not want their mail leaving the laptop
should not have to trust a hosted broker with a grant on it.  The local
rendezvous enforces the same scopes, the same schemas and the same
envelope; the only difference is that it is on the machine and it is
throwaway.

**Safari cannot use it** — localhost WebSockets are broken there — so on
Safari the local rendezvous is reached through the native host, over
native messaging, which is the same path the hosted tunnel takes.

---

## The broker, concretely

| concern | what it does |
|---|---|
| registry | surfaces, their schemas, their scopes, their schemes |
| grants | what the user granted, per surface, per extension instance; revocable |
| relay | typed frames, from one end to the other, with the scope check |
| replay | a short ring per tunnel, for resume after a gap |
| auth | the extension proves who it is; the client proves who it is |
| payload | **never sees one** |

**The extension authenticates with a device code**: it opens a page in the
user's browser, the user approves the surface there, and the extension
receives a long-lived secret.  The extension can be signed by the user
instead, which is stronger and is what the surfaces of kind 3 (the OS
app extensions) can do — an app extension is already signed and
sandboxed by the OS, and it can present its identity to the broker
through the OS's own keychain.

**The client authenticates with a token** it holds; the broker maps the
token to a set of surfaces it may subscribe to.

---

## The client interface

The far end is a **library first** and a CLI second, because the clients
that matter are agents and IDEs.

```rust
let mut surface = client.subscribe("conduit://mail/compose@v1").await?;
surface.on_event(|msg: mail::Message| { ... }).await?;
```

The library is typed over the *schema*: the payload of
`dev.conduit.mail.message/v1` arrives as a Rust struct generated from the
JSON Schema, and a payload that does not match its schema is a protocol
error, not a string that happens to be JSON.

---

## What is decided, and what is open

**Decided.** The two ends and the middle; the frame envelope; the scope
model; the scheme model; the per-platform transports; the loopback mode.

**Open.** Whether the broker may ever see a payload (today: no); whether a
grant may be delegated (today: no); how a surface's schema set is
published (today: the extension declares it, the broker records it); what
the client's authentication is in the first release (today: a token, with
a device-code flow for the extension side).

**Not designed yet.** The adapters — the per-app code that knows how to
read a Mail message or a browser tab.  They are the part that has to be
written per app family, and they are the part where the research in
`docs/research/extension-surfaces.md` becomes code.
