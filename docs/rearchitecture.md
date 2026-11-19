# The rearchitecture

This is the answer to the question *"how do we simplify or rearchitect the
core protocol with these constructs at a higher level, better understood
now?"* — and the record of what was done about it.

The constructs that made the question answerable are the ones the
surfaces work forced into existence: **the surface** (a named thing an app
offers), **the grant** (what the user allowed, and to whom), **the frame**
(the only thing that travels), **the near end** (the process that holds the
socket and the key), and **the plugin** (the code inside the app).

Most of the complexity was not in any of those.  It was in the seams
between them: a second vocabulary for the hop next to the app, two
implementations of the keys, two loops in the broker that were the same
loop twice, fifteen declarations of the same kind of thing, and a rule
about waiting that every plugin in every language had to obey.

The moves below take those seams out.  Four of them are done; two are not,
and this says so.

---

## 1. One vocabulary — **done**

**Before.** The tunnel carried frames: `{"t": ..., "s": ..., "c": ...,
"p": ...}`.  The hop between a plugin and the near end carried
*messages*: `{"op": "publish", "schema": ..., "payload": ...}`, plus an
`op: "status"` and an `op: "error"`.  Two shapes, two sets of field
names, and a rule that only the author of each remembered which was which.

**Now.** The hop carries **frames** — the same frames.  A plugin
publishes:

```json
{"t": "publish", "s": "dev.conduit.vscode.tab/v1", "c": "tabs", "seq": 0, "p": {"url": "https://example.test/"}}
```

and the near end seals `p` and sends the frame on.  The only difference
between the hop and the tunnel is that the payload of a published frame
is plaintext on the hop, because the near end is the one that seals it.

The one exception is named: the `connect` frame says where the broker is
and what secret may be presented on it, for a plugin that did the device
flow and handed the result to the process that holds the socket.  It
never goes near the tunnel, and **the broker refuses it** — there is a
test for that.

The dead frame kinds went with it: `unsubscribe` and `grant` were in the
registry and in the crate, and nothing sent them.

**The payload is JSON now.**  The envelope carried a *string* of JSON
that the other side parsed again; it carries JSON.

## 2. One implementation of the keys — **done**

**Before.** `conduit-client` had its own copy of the sealing: its own
key pair, its own nonce prefix, its own sequence numbers, its own copy
of the hello, and its own idea of which direction a key was for.
`conduit-near-end` had another.  They agreed, because the cross-language
vector test checked that they agreed — but they agreed *by hand*.

**Now.** Both use `conduit_protocol::end::End`: one key pair, one
sequence, one hello, one direction rule.  The client lost about a
hundred lines and the near end's sealing is the client's sealing.

The browser still has its own implementation, in JavaScript, because a
browser extension cannot run Rust — and the vector test in a real
Chromium still checks that the two agree, byte for byte.  That test is
the point: two implementations that are *proved* to agree are fine; two
implementations that agree by hand are not.

## 3. One loop carries both ends — **done**

**Before.** The broker had `serve_extension` and `serve_client`: two
loops, each reading a socket, parsing a frame, checking it against the
grant, and forwarding it.  The rules of the grant were written out
twice, and the two copies had drifted.

**Now.** There is one loop, `serve_end`, and it says which end it is
carrying.  The difference between the ends is one verb:

| the frame | the extension | the client |
|---|---|---|
| `publish` | needs `read` — what the app knows | needs `write` — what it wants done |
| `call` | — | needs `call` |

**A behaviour changed with it**, and it is the behaviour that was
written down: the extension leaving ends the tunnel, and the client
leaving does not.  The merged loop closed the tunnel for both, and a
client that left took the extension down with it.  The test that says
so caught it — which is the argument for merging the loops and having
the tests.

## 4. One declaration of every surface — **done**

**Before.** Fifteen `surface.json` files, one per surface, and nothing
checked that they agreed with each other or with anything.

**Now.** `spec/surfaces.json` is the registry.  The near end is told
about a surface from there, the checks read it, and the tests check it:
every surface in the tree is in the registry, the names and the
identities are each used once, the scopes are scopes the protocol knows,
and the identifiers are the ones the crate makes.

## 5. The verbs — **half done**

**Done.** The broker's rules are one table keyed by the end (move 3),
and `read`/`write`/`call`/`admin` are the only verbs the protocol
knows; the registry's scopes are checked against them.

**Not done.** A surface's declaration still lists *all* the scopes it
needs in one list, without saying which side needs which.  The mail
surface wants `mail.compose.read` and `mail.messages.read`; the client
that wants to *write* a draft needs `mail.compose.write`, and nothing in
the declaration says that the two are different kinds of thing.  The
grant is per `(surface, instance)` and carries a flat set of scopes, so
"read the message" and "write the draft" are the same sort of scope.

The move: the declaration says what the surface *offers* — a read
surface, a write surface, a call surface — and the grant says what the
user allowed of each.  The broker's check then becomes "does the frame
belong to a surface the user allowed", rather than "does the grant
happen to contain a scope with this verb somewhere".

## 6. A grant covers a set of surfaces — **not done**

**Before.** A grant is for one `(surface, instance)` pair, and the
browser's popup runs a **device flow per surface**: one code, one
approval, one secret, for each surface.  A browser extension with three
surfaces asks the user three times.

**Now.** Still one surface per grant, and still one device flow per
surface.  The popup's flow is per surface and the broker's registry is
keyed by surface.

**The move.** The device flow asks for a *set* of surfaces: the user
sees one page that names them all, approves once, and the secret covers
all of them.  The grant carries the set.  The broker's registry becomes
a grant per instance, with a set of surfaces under it, and the tunnel
lookup becomes "the tunnel of this surface under this instance".

This is the move that touches the broker's registry, the device flow,
the approval page and the browser's popup — and it is the one where the
security argument has to be made properly rather than assumed: the
approval page must name every surface in the set, and the set is what
the user sees.

---

## What did not change, and must not

- **The broker never opens a payload.**  It carries the envelope and
  nothing else.  A frame's `seq` is the sender's, because that number is
  inside the nonce; the broker renumbers nothing.
- **The payload is sealed end to end**, under `p256-aes256gcm/v1`: ECDH
  over P-256, HKDF-SHA256 with the RFC 5869 salt written out
  explicitly, two directional keys, AES-256-GCM with a nonce of four
  random bytes and an eight-byte sequence number that is also the
  additional data.
- **The plugin holds no key and opens no socket to the broker.**  It
  publishes plaintext to the near end and nothing else.
- **The approval endpoint sits behind the user's session.**  The code is
  the only thing standing between an approval and a stranger.
- **The surfaces that were never run say so**, in those words.  Swift,
  PHP and Lua are not compiled on the machine that wrote this, and the
  four surfaces without a check still say `not run => not verified`.

## The rule that came out of it

**A plugin does not wait for the tunnel to be up.**  It publishes when it
has something to say, and the near end holds what it published until the
two ends have agreed their keys, and sends it then.

The waiting used to be in every plugin in every language: a status
message, a poll, a timeout, and a queue for what could not be sent yet.
It was a race with a sleep in it, and the Apple Mail extension, the
WordPress plugin and the Neovim plugin each had their own version of it.

The near end holds the frames now.  The near end is one process, in one
language, with one rule — and a plugin is a publisher and a reader, and
nothing else.
