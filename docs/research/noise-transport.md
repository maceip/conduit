# noise transport, no tls

conduit with plain `ws://` and the Noise Protocol Framework doing the crypto. first written 2026-11-19.

question: *can the tunnel drop tls entirely and still be confidential + authenticated on every hop, with the broker still blind to payloads?* yes, with 2 noise layers and keys pinned at pairing. browsers allow it from extension origins; the human-facing approval page is the 1 piece noise can't cover.

anything marked *unverified* wasn't checked against a source or run in this pass.

---

## 1. recommendation

- 2 layers, both noise, no tls required:
  - hop (each end ↔ broker) = `Noise_IKpsk2_25519_AESGCM_SHA256`. the end ships with the broker's static key pinned; its own static key is what pairing binds the grant to. replaces wss + the bearer secret in the url.
  - end to end (extension ↔ client, relayed by the broker) = `Noise_XX_25519_AESGCM_SHA256`, then pin the peer's static after a short code the user compares on both ends. replaces `p256-aes256gcm/v1`.
- scheme names: `noise-ikpsk2-25519-aesgcm-sha256/v1` (hop), `noise-xx-25519-aesgcm-sha256/v1` (end to end).
- AESGCM + SHA256 => everything runs on browser WebCrypto; no ChaCha in the browser needed. 25519 => X25519 in WebCrypto, which now works with fixed private keys via `pkcs8` or `jwk` import (§4).
- keep tls as an optional outer wrapper, never as the security layer: same noise bytes inside `wss://` when a network eats `ws://`. WhatsApp Web already runs noise inside wss.
- rust side = `snow`; js side = a ~150-line noise state machine on WebCrypto, same as the throwaway one in §6, checked against snow + public vectors.

---

## 2. why 2 layers

the broker has to read routing metadata (`t`, `s`, `c`, `seq`, scopes) and must never read `p`. 1 layer can't do both.

- hop layer only => broker reads payloads. no.
- end-to-end layer only => over `ws://` the envelope, the grant, scopes, surface names and any credential are plaintext + modifiable by anyone on path. no.
- both => network sees ciphertext; broker sees envelope; only the 2 ends see payloads. same shape as libp2p noise over a relay.

cost = 2 AEADs per frame + 1 more round trip at session start. fine at 200 frames/s.

### what this fixes that exists today

`hello` carries each end's P-256 public key through the broker unauthenticated (`clients/shared/background.js` `meet()`, `crates/conduit-client/src/lib.rs` `hello_payload()`). a broker that swaps both keys is in the middle and opens everything. "the broker never opens it" holds only against a passive broker today. any e2e scheme, noise or not, needs the ends to authenticate each other's keys by a path the broker doesn't control => the short-code check in §3.2.

also: the 256-bit device secret + the client token are both issued by the broker => they can authenticate an end *to the broker*, never an end to the other end. they're fine as hop psks, useless as e2e keys.

---

## 3. handshake sketch

### 3.1 hop: end ↔ broker, `Noise_IKpsk2_25519_AESGCM_SHA256`

why IK: the end already knows who it's calling (pinned broker key) => 1 round trip, initiator identity encrypted, mutual auth after msg 2. WireGuard runs exactly this pattern (`Noise_IKpsk2_25519_ChaChaPoly_BLAKE2s`).

why psk2: the device secret (extension) or the token (client) becomes the psk => a stolen static key alone doesn't open a tunnel, and a future break of X25519 doesn't expose recorded hop traffic to a network observer (the broker knows the psk anyway). same reasoning WireGuard gives.

```
pairing (no psk yet; Noise_IK_25519_AESGCM_SHA256 to the same pinned broker key)
  end generates static s_end once; private key = non-extractable CryptoKey in IndexedDB
  -> e, es, s, ss   payload {op: "device/code", surface, scopes}
  <- e, ee, se      payload {code, url, expires_in}
  user approves code =>  grant bound to s_end.pub   (code theft is useless: redeeming needs s_end)
  ...poll over a fresh IK session: {op: "device/secret", code}  =>  {secret, grant}

tunnel (every new socket)
  ws://broker.example/v1/tunnel           <- no query string; the url is plaintext
  -> e, es, s, ss       payload = hello  {role, surface, resume, ts}      ts = replay guard, as WireGuard's TAI64N
  <- e, ee, se, psk     payload = welcome {grant, resume point}
  then: every ws binary message = 1 noise transport message = 1 JSON envelope
```

hello + welcome fold into the handshake => the socket is useful after 1 round trip. msg 1 payload is encrypted but replayable and not forward secret (IK property, noise spec §7) => nothing in it that matters if replayed; broker drops `ts` it has seen or that's > 30s off.

### 3.2 end to end: extension ↔ client, `Noise_XX_25519_AESGCM_SHA256`

why XX: neither end can trust the broker to tell it the other's key, so both statics travel in the handshake, then get checked against a pin. 1.5 round trips through the broker. KK (1 round trip) is an optimization once pins exist; *open*.

```
prologue = "conduit/e2e/v1" || surface || channel        binds the session to where it runs
-> e                          frame {t: "noise", s: "dev.conduit.noise/v1", c, seq, p: msg1}
<- e, ee, s, es               frame {t: "noise", ...}
-> s, se                      frame {t: "noise", ...}
first contact: both ends show a short code from h (handshake hash, noise §11.2); user compares
  extension popup + cli => same code => each pins the other's static
later: remote static != pin => refuse + tell the user (ssh known_hosts, minus the silent tofu)
```

### 3.3 transport messages and `seq`

conduit's ring replays frames after a reconnect => the receiver sees repeats + gaps. noise §11.4 covers this: send `n` with the message, receiver calls `SetNonce(n)`, rejects any `n` it already opened.

- `n` = the frame's `seq` (already per sender, already never renumbered by the broker).
- AD = canonical bytes of `t|s|c|i|seq` => the broker can't move a sealed payload to another type or channel. today only `seq` is AD.
- receiver keeps highest `seq` + a window bitmap, WireGuard style.
- e2e cipher states must outlive sockets, or ring replays from before a reconnect can't be opened => persist them (§7).

---

## 4. browser constraints, checked

### X25519 in WebCrypto

| | what | source |
|---|---|---|
| Chrome / Edge | X25519 from 133 | MDN / chromestatus via search (*unverified* directly); ran here on Chromium 153 + Chrome 157 |
| Firefox | from 130 | MDN via search (*unverified*) |
| Safari | from 17 | MDN via search (*unverified*); not run here |
| Node | works on 24.15.0 | ran here |

the repo's "X25519 won't take a raw private key" is format-specific. `importKey("raw", …)` is public-key-only for X25519. private keys go in as `pkcs8` (fixed 16-byte DER prefix `302e020100300506032b656e04220420` + the 32 bytes) or `jwk` (`kty: OKP`, `crv: X25519`, `d`, `x`). both work with `deriveBits` (§6) => P-256 isn't forced anymore, and a fixed-key test vector runs in Node too.

noise only defines 25519 + 448 for DH. P-256 noise would be a private name outside the spec (BOLT 8 does that with secp256k1) => drop P-256 for the noise schemes.

### AESGCM + SHA256

valid noise cipher + hash (spec §12.4, §12.5). nonce = 32 bits of zeros + big-endian `n` => 1 `DataView.setBigUint64` call; AES-GCM, HMAC-SHA256 and SHA-256 are all WebCrypto. WhatsApp and Nebula both run AESGCM noise.

max noise message = 65535 bytes incl. the 16-byte tag. today's limit is 64 KiB sealed. with an e2e payload base64url'd inside a JSON envelope inside a hop message => ≈ 48 KiB plaintext per frame. either lower the limit, chunk, or make the envelope binary; *open*.

### can an extension open `ws://` to a non-loopback host?

ran here (Playwright Chromium 153 headless, unpacked MV3, no `host_permissions`, no CSP override):

| from | to | result |
|---|---|---|
| MV3 service worker | `ws://10.0.0.242:9555` (lan) | open; full noise XX / IK / IKpsk2 with snow |
| MV3 extension page | `ws://10.0.0.242:9555` | open; same |
| MV3 extension page | `ws://neverssl.com/` (public, port 443 doesn't answer) | request went out plaintext: `Unexpected response code: 200` => no mixed-content block, no upgrade |
| MV3 service worker | `ws://example.com/`, `ws://neverssl.com/` | errors at a timing close to the page's for example.com; neverssl hung once => *unverified* for public hosts from the worker |
| `https://example.com` page | `ws://example.com/` | blocked: `Mixed Content … This request has been blocked`, `SecurityError` |
| `https://example.com` page | `ws://10.0.0.242:9555` | mixed-content warning only, then `net::ERR_BLOCKED_BY_LOCAL_NETWORK_ACCESS_CHECKS` |

=> in chromium an extension origin is a secure context (`isSecureContext: true`) yet isn't held to mixed content for `ws://`. extensions skipped the local network access check here too; a web page doesn't.

- Chrome's MV3 default CSP has no `upgrade-insecure-requests` (matches the neverssl result).
- Firefox's MV3 default CSP is `script-src 'self'; upgrade-insecure-requests;` => `ws://` silently becomes `wss://` unless the manifest overrides `extension_pages` (MDN). AMO policy also says user data must be "transmitted securely"; whether noise over `ws://` passes review is *unverified*.
- branded Chrome 157 ignored `--load-extension` (removed in branded builds from 137 per search, *unverified* link) => the worker rows ran on Playwright Chromium only.
- Safari: not run. the extension never holds the socket anyway (§2 of `extension-surfaces.md`), the native host does. the host uses tokio-tungstenite sockets; ATS governs the URL loading system (`URLSession`), so raw sockets aren't held to it (Apple docs; *unverified* on a real Safari build). a `URLSessionWebSocketTask` with `ws://` would need an ATS exception.

---

## 5. what you lose without tls, and the cover

| concern | tls gives | noise plan |
|---|---|---|
| server auth | WebPKI cert for the hostname | broker static pinned in the extension build / client config; IK fails closed on a wrong key. rotation = ship 2 keys or sign the next key with an Ed25519 root (WebCrypto Ed25519: Chrome 137+, *unverified*); *open* |
| client auth | rarely used | end static + psk in the handshake; grant bound to the static at pairing |
| bearer secret on the wire | hidden by tls | gone: becomes a psk, never sent |
| url + headers | encrypted | plaintext: `GET /v1/tunnel` + Host. no surface, token or instance in the url |
| passive observer | ciphertext | ciphertext; initiator identity hidden (IK/XX) |
| replay | tls record layer | hop: per-direction nonces; IK msg 1 replay => `ts` check. e2e: `seq` window |
| middleboxes | opaque 443 | `ws://` on 80 meets transparent proxies that strip `Upgrade` or block; captive portals; dpi flags unknown framing => fallback: same noise inside `wss://` |
| human approval page | https | not covered: a browser can't speak noise to a page. on-path tampering could change the scopes shown. options: keep that page on https, or move approval into the client cli (which already has a noise channel to the broker) + echo the grant in the extension popup |
| DoS | tls handshake cost is the server's too | 1 DH-heavy msg per connect; ws upgrade already needs a tcp handshake (no spoofed ips); rate limit per ip. WireGuard's cookie reply exists if needed |

---

## 6. experiment (throwaway, outside the repo, not committed)

- responder: rust `snow` 0.10.0 + `tungstenite` 0.30, plain `ws://0.0.0.0:9555`, static = RFC 7748 alice key. prints its handshake hash.
- initiator: ~150 lines of js on WebCrypto only (X25519, HMAC-SHA256, AES-GCM); patterns XX, IK, IKpsk2, all `_25519_AESGCM_SHA256`; static = RFC 7748 bob key imported as pkcs8.
- each run: handshake, then 1 transport message each way (`ping over noise` => snow uppercases it back).

results:

- `importKey("raw", x25519 private, ["deriveBits"])`: refused everywhere. Node 24.15.0: `SyntaxError: Unsupported key usage for a X25519 key`. Chromium 153 / Chrome 157: `SyntaxError: Cannot create a key using the specified key usages.`
- pkcs8 import: public key + shared secret match RFC 7748 §6.1 exactly; jwk import: shared secret matches. Node + Chromium + Chrome.
- noise interop: 39 handshakes (Node: 3; Chromium 153 http page, LAN-origin https page, MV3 service worker, MV3 extension page; Chrome 157 http + https page; each against `ws://127.0.0.1` + `ws://10.0.0.242`, each XX + IK + IKpsk2) => all completed, payloads in both directions opened, and the js handshake hash matched snow's in every one of the 36 browser runs; the 3 Node runs matched too.
- handshake time: 1-8 ms on loopback / lan.
- the LAN-origin https page reached `ws://10.0.0.242` with cert errors ignored; that's the local-network mixed-content exemption, so it says nothing about public hosts.

---

## 7. what changes in frames + hello

- transport: `ws://` (or `wss://` as a wrapper) to `/v1/tunnel`, no query string. first 2 binary messages = hop handshake; after that binary only, 1 noise transport message per envelope. text frames gone.
- `hello` / `welcome`: become the hop handshake payloads. `hello.crypto.{public, prefix}` gone; `hello.schemes` lists e2e schemes the end speaks; new `ts`.
- new frame type `noise` (end → end, relayed, no scope needed beyond the tunnel) + schema `dev.conduit.noise/v1` => `spec/frames.json` + `spec/schemas/`.
- `p`: base64url of the e2e noise ciphertext; `n = seq`, AD = `t|s|c|i|seq`.
- grant: `"schemes": ["noise-xx-25519-aesgcm-sha256/v1"]`; broker records the hop scheme per end.
- pairing endpoints: same ops, carried over the IK session; `/v1/device/secret` returns the psk, bound to the end's static.
- `p256-aes256gcm/v1`: keep for a transition, mark as unauthenticated against an active broker.

---

## 8. prior art

| who | pattern | over | notes |
|---|---|---|---|
| WhatsApp Web | `Noise_XX_25519_AESGCM_SHA256` | `wss://web.whatsapp.com/ws/chat` | noise inside tls from a browser; whitepaper: "Noise Pipes with Curve25519, AES-GCM, and SHA256"; noise pipes = XX + cached-key IK + XXfallback (spec §10). string checked in whatsmeow `socket/constants.go` |
| WireGuard | `Noise_IKpsk2_25519_ChaChaPoly_BLAKE2s` | udp | pinned peer keys, psk, TAI64N timestamp vs replay, cookies vs DoS |
| Lightning BOLT 8 | `Noise_XK_secp256k1_ChaChaPoly_SHA256` | tcp | non-spec curve; initiator knows the node key from the node id |
| Nebula | IX with psk0 (`HandshakeIXPSK0`) | udp | cipher config: `aes` or `chachapoly`, same on every node; statics carried in certs |
| Signal (libsignal attest) | `Noise_NK_25519_ChaChaPoly_SHA256` (+ `NKhfs`) | websocket | server static comes out of an enclave attestation => key pinning by evidence instead of WebPKI |
| libp2p | `Noise_XX_25519_ChaChaPoly_SHA256` | tcp, websocket, relays | e2e through relays the same way conduit goes through the broker |

## 9. libraries

| lib | version (checked) | fits? |
|---|---|---|
| `snow` (rust) | 0.10.0, MIT/Apache-2.0 | yes. 25519, AESGCM + ChaChaPoly, psk modifiers; ran here. a Trail of Bits audit funded by 1Password was reported (search; report not read => *unverified*) |
| WebCrypto, hand-rolled | n/a | yes: what §6 ran. async, native AES-GCM, non-extractable keys. needs vector tests |
| `@chainsafe/libp2p-noise` | 17.0.0 | no: only `Noise_XX_25519_ChaChaPoly_SHA256`, tied to libp2p identity + framing |
| `noise-handshake` (holepunch) | 4.2.1 | maybe: IK etc., depends on `sodium-universal` => ChaCha/libsodium, not WebCrypto |
| `noise-c.wasm` | 0.4.0, last published 2022-05 | no: stale |
| `@noble/curves` + `@noble/ciphers` | 2.4.0 | primitives only; useful if ChaChaPoly is ever wanted in js |

---

## 10. open questions

- e2e cipher state across service-worker deaths: `chrome.storage.session` (memory, raw key bytes) vs non-extractable AES-GCM `CryptoKey` in IndexedDB (disk, survives restarts)? and when does an e2e session rekey (noise `Rekey()` per N frames, or a fresh XX)?
- KK after pinning (1 round trip) vs always XX (1 state machine)?
- short code from `h`: how many digits/words, and where the cli shows it.
- approval page: keep https for it alone, or move approval into the cli?
- broker key rotation + revocation without WebPKI.
- frame limit vs 65535-byte noise messages: chunk, binary envelope, or lower the limit.
- store review: does Chrome Web Store / AMO accept noise over `ws://` as "transmitted securely"? *unverified*.
- Safari: native host does both layers in rust (snow), or e2e stays in extension js (WebCrypto X25519, Safari 17+)? not run.
- does the MV3 service worker reach a public `ws://` host the way the extension page does? page: yes; worker: *unverified* (§4).
- seen while reading, not tested: the rust client sends `crypto.public` as hex, `background.js` sends base64url.

---

## sources

- [Noise Protocol Framework, revision 34](https://noiseprotocol.org/noise.html): §7 (pattern properties), §11.2 (channel binding), §11.4 (out-of-order transport messages), §12 (25519, AESGCM, SHA256), 65535-byte limit
- [RFC 7748](https://www.rfc-editor.org/rfc/rfc7748): X25519 + the §6.1 vectors
- [Secure Curves in WebCrypto](https://wicg.github.io/webcrypto-secure-curves/), [MDN: SubtleCrypto.importKey](https://developer.mozilla.org/en-US/docs/Web/API/SubtleCrypto/importKey)
- [Mixed Content spec](https://w3c.github.io/webappsec-mixed-content/)
- [Chrome: local network access](https://developer.chrome.com/blog/local-network-access)
- [Chrome: WebSockets in extension service workers](https://developer.chrome.com/docs/extensions/how-to/web-platform/websockets)
- [MDN: extension CSP (Firefox MV3 default `upgrade-insecure-requests`)](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json/content_security_policy)
- [Apple: NSAppTransportSecurity](https://developer.apple.com/documentation/bundleresources/information-property-list/nsapptransportsecurity)
- [snow](https://github.com/mcginty/snow)
- [WireGuard protocol](https://www.wireguard.com/protocol/)
- [BOLT 8](https://github.com/lightning/bolts/blob/master/08-transport.md)
- [Nebula](https://github.com/slackhq/nebula)
- [WhatsApp security whitepaper](https://www.whatsapp.com/security/WhatsApp-Security-Whitepaper.pdf), [whatsmeow](https://github.com/tulir/whatsmeow), [Baileys](https://github.com/WhiskeySockets/Baileys)
- [libsignal attest](https://github.com/signalapp/libsignal/tree/main/rust/attest)
- [libp2p noise spec](https://github.com/libp2p/specs/blob/master/noise/README.md), [js-libp2p-noise](https://github.com/ChainSafe/js-libp2p-noise)
- [noise-handshake](https://github.com/holepunchto/noise-handshake), [noise-c.wasm](https://github.com/nazar-pc/noise-c.wasm), [noble-ciphers](https://github.com/paulmillr/noble-ciphers)
