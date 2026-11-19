# protocol

frames, scopes, schemes. first written 2026-11-18. normative schemas live in
`spec/`; this doc says what they mean.

## transport

1 WebSocket per end. the broker speaks the same envelope on both.

```
wss://broker.example/v1/tunnel?role=extension&surface=dev.conduit.mail%2Fcompose%2Fv1
wss://broker.example/v1/tunnel?role=client&token=...
```

extension auths with a long-lived secret from the device-code flow; client
auths with a token. after that both send the same frames.

## envelope

every frame on the wire is 1 JSON object with these fields and no others
(`spec/schemas/envelope.schema.json` is the authority):

| field | type | meaning |
|---|---|---|
| `t` | string | frame type, from the registry in `spec/frames.json` |
| `s` | string | payload schema id, `reverse-dns/version` |
| `c` | string | channel the frame belongs to |
| `i` | integer | call id, for frames that answer a call |
| `seq` | integer | sequence number of this frame in the tunnel |
| `p` | string | sealed payload |

an envelope that doesn't match the schema is a protocol error: frame
refused, counted against the peer; after 10 refusals the connection is
closed.

## frame types

| type | direction | payload schema | meaning |
|---|---|---|---|
| `hello` | both | `dev.conduit.hello/v1` | who i am, what i can do, what i have |
| `welcome` | broker → end | `dev.conduit.welcome/v1` | grant, schemes, resume point |
| `subscribe` | client → broker | `dev.conduit.subscribe/v1` | i want this channel with these scopes |
| `subscribed` | broker → client | `dev.conduit.subscribed/v1` | granted; here's the resume point |
| `unsubscribe` | client → broker | `dev.conduit.unsubscribe/v1` | stop that channel |
| `publish` | extension → broker | `dev.conduit.publish/v1` | an event for that channel |
| `call` | client → extension | `dev.conduit.call/v1` | do this |
| `result` | extension → broker | `dev.conduit.result/v1` | answer to that call |
| `error` | either | `dev.conduit.error/v1` | it failed + why |
| `ping` | either | `dev.conduit.ping/v1` | i'm here |
| `pong` | either | `dev.conduit.pong/v1` | i'm here too |
| `grant` | broker → extension | `dev.conduit.grant/v1` | the user changed the grant |

`ping` keeps a chromium worker alive. from Chrome 116, sending or receiving
it resets the service worker's 30s idle timer; an idle socket doesn't. the
client sends one every 20s and the broker answers `pong` without waking
anything.

## scopes

scope name = `namespace.verb`; the namespace belongs to the surface that
defines it. 2 surfaces can't define the same scope name, and a client can't
widen a grant by asking twice.

the verbs are the entire vocabulary:

| verb | what it allows |
|---|---|
| `<surface>.read` | extension publishes, client receives |
| `<surface>.write` | client publishes, extension receives |
| `<surface>.call` | client can invoke calls on the surface |
| `<surface>.admin` | client can change the surface's own settings |

`admin` is the only scope that can change anything; the broker refuses to
grant it to a token the user didn't create in the surface's own consent
surface.

## grant

### how an extension gets one

the broker doesn't hand out a grant just because an extension asked. the
extension asks for a code, shows it to the user, the user approves it on the
broker's own page, and only then does the extension get a secret.

```
POST /v1/device/code     {surface, scopes}   ->  {code, url, expires_in}
POST /v1/device/approve  {code}              ->  200, or 404 if it is not there
POST /v1/device/secret   {code, instance}    ->  the secret, or 403
GET  /v1/device/approve-page?code=…          ->  the page the user sees
```

code = 192 bits of randomness, lives 10 minutes. secret = 256 bits; it's
what the extension presents on the tunnel. the grant that comes with it is
the one the user approved: the surface + scopes named on the page.

the approval endpoint has to sit behind the user's session in anything but a
bootstrap. here the code itself is the only thing between an approval and a
stranger; same tradeoff the OAuth device flow makes.

### the grant itself

the grant is the user's decision and lives with the extension:

```json
{
  "surface": "dev.conduit.mail/compose/v1",
  "instance": "4f9c…",
  "scopes": ["mail.compose.read", "mail.compose.write"],
  "schemes": ["p256-aes256gcm/v1"],
  "expires": null
}
```

the broker stores the grant; the extension can ask to change it; the client
hears about the change via a `grant` frame and must re-subscribe if it lost
a scope.

## resume

every frame carries a `seq`. the broker keeps the last 256 frames of each
tunnel in a ring. a reconnecting end sends its last seen `seq` in `hello`;
the broker replays what it has and says in `welcome` where the replay
stopped. if the gap is bigger than the ring, `welcome` says so and the end
must take a fresh snapshot => the client must never assume it has seen
everything.

## errors

| code | meaning |
|---|---|
| `unauthorized` | credential isn't good |
| `scope_denied` | frame or subscription names a scope the grant doesn't cover |
| `unknown_type` | frame type isn't in the registry |
| `schema_invalid` | payload doesn't match the schema it names |
| `schema_unknown` | schema id isn't one the surface offers |
| `surface_unknown` | no such surface |
| `expired` | grant expired |
| `too_large` | frame is over the size limit (64 KiB sealed) |

an error frame carries the code, a human-readable message, and the id of the
frame it answers, if any.

## sealed payload

implemented. scheme is `p256-aes256gcm/v1`; the broker carries the payload
as an opaque string it never opens.

* ECDH over P-256 for key agreement, at hello, between the two ends; the
  broker isn't a party to it. hello carries each end's public key and its
  nonce prefix in `crypto.public` / `crypto.prefix`, base64url without
  padding; hex is refused.
* HKDF-SHA256 over the shared secret, salt = the string of zeros RFC 5869
  names, => 2 directional keys: one for what the extension sends, one for
  what the client sends, so a frame can't be replayed backwards.
* AES-256-GCM for the payload; nonce = 4 random bytes + the frame's `seq`,
  and `seq` is also the additional data => ordering is authenticated along
  with content.
* sealed blob is base64url without padding, in the `p` field.

2 choices were forced by what the browser can do. browser WebCrypto has no
ChaCha20-Poly1305, and its X25519 won't take a raw private key with the
usage a derivation needs => AES-GCM + P-256. the browser and the Rust are
checked against the same vector: same keys, nonce, payload => same
ciphertext, byte for byte.

the broker renumbers nothing. a frame's `seq` is the sending end's, because
that number is part of the nonce; a frame the broker renumbered couldn't be
opened.

no tls, noise on both hops instead (proposed `noise-xx-25519-aesgcm-sha256/v1`) =>
[`research/noise-transport.md`](research/noise-transport.md)

a surface that declares `none` is only reachable over a loopback tunnel,
never through the hosted broker; the broker refuses to carry a `none` grant
that arrives over the network.

## size and rate

* a frame is at most 64 KiB sealed; the broker refuses more.
* the broker enforces 200 frames/s per tunnel, both ends together, with a
  token bucket (`RateLimit` in `crates/conduit-broker/src/relay.rs`); it
  drops the excess instead of closing and counts what it dropped.
* a client that wants more asks for a larger window in the `subscribe`
  frame; the broker grants what the extension's rate allows. not built yet
  => `window` is ignored and every tunnel gets 200/s.
