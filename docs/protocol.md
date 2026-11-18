# The protocol

**The frames, the scopes, the schemes.**  First written 2026-11-18.
The normative schemas live in `spec/`; this document says what they mean.

## Transport

One WebSocket per end.  The broker speaks the same envelope on both.

```
wss://broker.example/v1/tunnel?role=extension&surface=dev.conduit.mail%2Fcompose%2Fv1
wss://broker.example/v1/tunnel?role=client&token=...
```

The extension authenticates with a long-lived secret it obtained from the
device-code flow; the client authenticates with a token.  Both send the
same frames afterwards.

## The envelope

Every frame on the wire is one JSON object with these fields and no others
(`spec/schemas/envelope.schema.json` is the authority):

| field | type | meaning |
|---|---|---|
| `t` | string | the **frame type**, from the registry in `spec/frames.json` |
| `s` | string | the **schema id** of the payload, `reverse-dns/version` |
| `c` | string | the **channel** the frame belongs to |
| `i` | integer | the **call id**, for frames that answer a call |
| `seq` | integer | the **sequence number** of this frame in the tunnel |
| `p` | string | the **sealed payload** |

An envelope that does not match the schema is a protocol error: the frame
is refused, the connection is counted against the peer, and after ten
refusals the connection is closed.

## The frame types

| type | direction | payload schema | meaning |
|---|---|---|---|
| `hello` | both | `dev.conduit.hello/v1` | who I am, what I can do, what I have |
| `welcome` | broker → end | `dev.conduit.welcome/v1` | the grant, the schemes, the resume point |
| `subscribe` | client → broker | `dev.conduit.subscribe/v1` | I want this channel, with these scopes |
| `subscribed` | broker → client | `dev.conduit.subscribed/v1` | granted, here is the resume point |
| `unsubscribe` | client → broker | `dev.conduit.unsubscribe/v1` | stop that channel |
| `publish` | extension → broker | `dev.conduit.publish/v1` | here is an event, for that channel |
| `call` | client → extension | `dev.conduit.call/v1` | do this thing |
| `result` | extension → broker | `dev.conduit.result/v1` | the answer to that call |
| `error` | either | `dev.conduit.error/v1` | it did not work, and why |
| `ping` | either | `dev.conduit.ping/v1` | I am here |
| `pong` | either | `dev.conduit.pong/v1` | I am here too |
| `grant` | broker → extension | `dev.conduit.grant/v1` | the user changed the grant |

**`ping` is the frame that keeps a Chromium worker alive.**  From Chrome
116, sending or receiving it resets the 30-second idle timer of the
service worker; an idle socket does not.  The client sends one every 20
seconds, and the broker answers with `pong` without waking anything.

## The scopes

A scope name is `namespace.verb`, and the namespace belongs to the surface
that defines it.  Two surfaces cannot define the same scope name, and a
client cannot widen a grant by asking twice.

The verbs are the whole vocabulary:

| verb | what it allows |
|---|---|
| `<surface>.read` | the extension publishes, the client receives |
| `<surface>.write` | the client publishes, the extension receives |
| `<surface>.call` | the client may invoke calls on the surface |
| `<surface>.admin` | the client may change the surface's own settings |

`admin` is the only scope that can change anything, and the broker refuses
to grant it to a token that was not created by the user in the surface's
own consent surface.

## The grant

### How an extension gets one

The broker does not hand out a grant because an extension asked.  The
extension asks for a **code**, shows it to the user, the user approves
it in the broker's own page, and only then does the extension get a
**secret**.

```
POST /v1/device/code     {surface, scopes}   ->  {code, url, expires_in}
POST /v1/device/approve  {code}              ->  200, or 404 if it is not there
POST /v1/device/secret   {code, instance}    ->  the secret, or 403
GET  /v1/device/approve-page?code=…          ->  the page the user sees
```

The code is 192 bits of randomness and lives ten minutes.  The secret is
256 bits, and it is what the extension presents on the tunnel; the grant
that comes with it is the one the user approved — the surface and the
scopes that were named in the page.

**The approval endpoint has to sit behind the user's session** in
anything but a bootstrap.  Here the code itself is the only thing that
stands between an approval and a stranger, which is the same bargain
the OAuth device flow makes.

### The grant itself

The grant is the user's decision, and it lives with the extension:

```json
{
  "surface": "dev.conduit.mail/compose/v1",
  "instance": "4f9c…",
  "scopes": ["mail.compose.read", "mail.compose.write"],
  "schemes": ["p256-aes256gcm/v1"],
  "expires": null
}
```

The broker stores the grant; the extension can ask to have it changed;
the client is told about the change with a `grant` frame and must
re-subscribe if it lost a scope.

## Resume

Every frame carries a `seq`.  The broker keeps the last 256 frames of each
tunnel in a ring.  A reconnecting end sends its last seen `seq` in the
`hello` frame; the broker replays what it has, and says in the `welcome`
frame where the replay stopped.  If the gap is larger than the ring, the
`welcome` says so and the end must take a fresh snapshot — **the client
must never assume it has seen everything**.

## Errors

| code | meaning |
|---|---|
| `unauthorized` | the credential is not good |
| `scope_denied` | the frame or the subscription names a scope the grant does not cover |
| `unknown_type` | the frame type is not in the registry |
| `schema_invalid` | the payload does not match the schema it names |
| `schema_unknown` | the schema id is not one the surface offers |
| `surface_unknown` | there is no such surface |
| `expired` | the grant expired |
| `too_large` | the frame is over the size limit (64 KiB sealed) |

An error frame carries the code, a human-readable message, and the id of
the frame it answers, if any.

## The sealed payload

**This part is implemented.**  The scheme is `p256-aes256gcm/v1`, and the
broker carries the payload as an opaque string it never opens.

* **ECDH over P-256** for the key agreement, at the hello, between the
  two ends — the broker is not a party to it.  The hello carries the
  public key of each end and the prefix of its nonces, both **base64url
  without padding** — the same encoding as the sealed blob, so a frame
  has one encoding and not two.
* **HKDF-SHA256** over the shared secret, with the string of zeros RFC
  5869 names as the salt, to make **two directional keys**: one for what
  the extension sends and one for what the client sends, so a frame
  cannot be replayed backwards.
* **AES-256-GCM** for the payload, with a nonce of four random bytes
  and the frame's `seq`, and the `seq` also as the additional data — so
  the ordering is authenticated as well as the content.
* The sealed blob is base64url without padding, in the `p` field.

**Two choices were forced by what the browser can actually do.**  The
browser's WebCrypto has no ChaCha20-Poly1305, and its X25519 will not
take a raw private key with the usage a derivation needs.  So the scheme
is AES-GCM and P-256, and the browser and the Rust are checked against
the same vector: the same keys, the same nonce, the same payload, and
therefore the same ciphertext, byte for byte.

**The broker renumbers nothing.**  The `seq` of a frame is the one of
the end that sent it, because that number is part of the nonce; a frame
the broker renumbered could not be opened.

A surface that declares `none` is only reachable over a loopback tunnel
and never through the hosted broker; the broker refuses to carry a `none`
grant that arrives over the network.

## Size and rate

* A frame is at most 64 KiB sealed, and the broker refuses more.
* The broker enforces 200 frames per second per tunnel, and drops the
  excess rather than closing.
* A client that wants more asks for a larger window in the `subscribe`
  frame; the broker grants what the extension's rate allows.
