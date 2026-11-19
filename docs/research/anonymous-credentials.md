# anonymous rate-limited credentials

side research, not a plan. first written 2026-11-19.

question: *can a client or extension prove it holds a valid grant + stay under a rate limit, without the broker linking its requests to an identity or to each other?*

short answer: the crypto exists and runs fast enough in a browser via wasm. it buys conduit little today: the broker routes 1 extension to 1 client, sees both IPs + both sockets, and a self-hosted broker has 1 user. park it; revisit if a hosted multi-tenant broker ships. details + a sketch below.

every status, version + date here was checked against the source listed in §9 on 2026-11-19. anything not checked is marked *unverified*.

---

## 1. what conduit has today

from `docs/protocol.md` + the code:

* pairing => device-code flow; the extension ends up with a 256-bit secret bound to 1 surface + its scopes. the broker stores `secret -> surface` (`crates/conduit-broker/src/devices.rs`) => every extension connection names its grant.
* client auth => a bearer `token` in the websocket query string (`crates/conduit-client/src/lib.rs`). the broker maps token -> surfaces (`registry.rs`).
* control frames (`hello`, `subscribe`, ...) are broker-readable; the broker parses `subscribe` scopes out of the frame (`relay.rs`). app payloads in `p` are sealed `p256-aes256gcm/v1`; the broker never opens them.
* replay ring = last 256 frames per tunnel end, keyed by the sender's `seq`.
* 200 frames/s per tunnel is in the spec. in the code i found only the `window` field in `spec/schemas/subscribe.schema.json`; no limiter in `crates/conduit-broker/src/` => the rate limit is spec-only right now.
* the extension's ECDH key pair lives as long as the worker (`state.keys` in `clients/shared/background.js`) => reconnects within 1 worker life show the broker the same public key.

so the broker can already link everything: grant secret, token, socket, IP, ECDH key, timing. any anonymous credential has to fix all of those or it fixes nothing.

---

## 2. the schemes

| scheme | what it gives | spec status (2026-11-19) | crypto | rust | js |
|---|---|---|---|---|---|
| Privacy Pass | single-use unlinkable tokens; issuer can't link issuance to redemption | RFC 9576 (architecture, informational), RFC 9577 (`PrivateToken` http auth, proposed standard), RFC 9578 (issuance, proposed standard); all June 2024 | type `0x0001` = VOPRF(P-384, SHA-384), RFC 9497; type `0x0002` = blind RSA 2048-bit, RFC 9474 | `privacypass` 0.1.0 (0.2.0-pre.4), `voprf` 0.5.0, `blind-rsa-signatures` 0.19.0 | `@cloudflare/privacypass-ts`, `voprf-ts`, `blindrsa-ts` |
| rate-limit tokens | per-origin rate limits on Privacy Pass issuance | `draft-ietf-privacypass-rate-limit-tokens-06`, WG document, expired | needs an attester + issuer split | none found | none found |
| batched tokens | many tokens per issuance | `draft-ietf-privacypass-batched-tokens-10`, submitted to IESG | same as RFC 9578 | none checked | `privacypass-ts` (draft -06) |
| ARC | 1 credential, up to N unlinkable presentations per context; presentations can run in parallel | `draft-ietf-privacypass-arc-crypto-01` + `-arc-protocol-01`; marked *dead WG document* 2026-07-22, expired 2026-09-03 | KVAC, P-256 ciphersuite | none on crates.io; python reference at `ietf-wg-privacypass/draft-arc` | none found |
| ACT | credential carries a hidden credit balance; spend k, get anonymous change; nullifier stops double spend; 1 presentation at a time | `draft-schlesinger-cfrg-act-01` + `draft-schlesinger-privacypass-act-01`, individual, expired 2026-08-17 | KVAC over privately verifiable BBS-style sig, ristretto255, RFC 9496 | `anonymous-credit-tokens` 0.4.2 (unaudited, says so) | none found |
| EC-ACT / MOLE | ACT rewritten as a credential for the MOLE architecture | `draft-schlesinger-mole-ec-act-00`, 2026-11-02, active; MOLE is at BoF stage | pairing-free BBS-style, EC-ACT(P-256, SHA-256) | none found | none found |
| Apple private access tokens | Privacy Pass where Apple attests the device and a 3rd party issues | WWDC22 session 10077; shipping since iOS 16 / macOS Ventura (per search, not checked against Apple docs) | RFC 9578 tokens | n/a | os-level only |
| Chrome private state tokens | origin-scoped trust bits, VOPRF-based (formerly trust tokens) | Google said in its Privacy Sandbox update it will "maintain Private State Tokens" while retiring most other sandbox apis (date per search: October 2025, *not checked on the page*) | VOPRF | n/a | web page api; *unverified* whether an extension service worker can call it |
| Signal zkgroup / KVAC | server issues + verifies credentials over group-element attributes; group membership hidden from the server | paper: Chase, Perrin, Zaverucha, eprint 2019/1416 (CCS 2020 per search) | algebraic MAC, ristretto255 | `libsignal/rust/zkcredential` + `zkgroup` (in-repo, not on crates.io); `kvac` 0.8.0 (docknetwork) | not checked |
| BBS | multi-message sig with selective disclosure | `draft-irtf-cfrg-bbs-signatures-15`, active RG document, posted 2026-11-19; sibling drafts `-bbs-blind-signatures-05`, `-bbs-per-verifier-linkability-05` | pairings, BLS12-381 | `bbs` 0.4.1 is from 2020 (hyperledger ursa) => stale | not checked |
| RLN | group member posts ≤ r messages per epoch; posting more leaks the secret key => slashing | not an IETF doc; PSE + vac docs; v1 -> v2 (per-member limits) -> v3 (variable epochs), versions per search | zk-SNARK + merkle tree + shamir shares, Poseidon | `rln` 3.1.0 (vacp2p/zerokit) | circom / zerokit wasm (*unverified for MV3*) |

the IETF thread moved in 2026: the privacypass WG stopped ARC on 2026-07-22 because it "has been unable to develop an attractive version of ARC with post-quantum unlinkability", and the newest credit-token work is EC-ACT inside MOLE. => nothing anonymous-and-rate-limited is near an RFC. building on any of it today = building on a moving draft.

ACT vs ARC, from the Cloudflare post (2025-10-30): ACT can't be presented twice at once; the first spend has to finish so the re-issued credential exists for the next. ARC was the parallel one, up to its presentation limit. that matters for a tunnel that reconnects.

---

## 3. what fits conduit's shape

### issuer = broker, after approval

the broker already gates on the device-code approval. it can also be the issuer + verifier, which is the KVAC case: privately verifiable, no pairings, no public issuer key to publish. ARC, ACT, EC-ACT + zkgroup are all KVAC.

* extension side => after `/v1/device/secret`, the extension asks for a credential, authenticated by the secret. issuance is linked to the grant (it has to be: that's where the grant check happens). later presentations aren't.
* client side => same, authenticated by the token.
* revoke => the broker stops issuing. credentials already out stay valid until their epoch ends.

### per-scope or per-grant

a credential has to say what it's good for, or the broker can't enforce scopes. 2 options:

1. 1 issuer key per (surface, scope set). simple; the scope set becomes public metadata => anonymity set = everyone with that exact scope set on that broker.
2. scopes as hidden attributes + selective disclosure per presentation (BBS-style). smaller leak, much more crypto, no current draft that conduit could just use.

1 is the only realistic one. it also means `admin` grants sit in a tiny set by design.

### per-epoch limits, and where the 200 frames/s belongs

i measured per-spend cost (§5): about 1 ms on the client + about 0.6-1 ms on the broker per presentation, 1.6-2.7 KB per proof. at 200 frames/s that's 120-200 ms of broker cpu per tunnel per second + up to ~545 KB/s of proofs. and inside 1 websocket the frames are linked anyway.

=> credentials go on the connection, never on frames. the per-frame limit stays a plain counter on the socket. what the credential limits is *connections per epoch* (or windows per epoch: 1 credit = 1 window of 200 frames/s for 1 session).

* ACT fits: balance = connection budget for the epoch; each `hello` spends 1; change comes back in `welcome`. sequential is fine because 1 tunnel end has 1 live socket.
* ARC would have fit too (N presentations per epoch, parallel), but it's dead in the WG.
* RLN fits the "limit per epoch without a counter" shape, but it needs a merkle registry + a SNARK prover in the extension + a slashing story. conduit has no stake to slash.

### replay ring, seq, sealed payloads

* ring + `seq` => unchanged inside a session. the ring is keyed by tunnel today; in anonymous mode it'd be keyed by a rendezvous id (§4). if that id rotates per epoch, resume can't cross an epoch boundary without the broker linking old id -> new id => at rollover the end gets `gap: true` + takes a fresh snapshot. that path already exists.
* nullifiers => 1 more set on the broker, per epoch, dropped at rollover. same lifetime idea as the ring.
* sealed payloads => untouched. the credential is for the broker, so it rides in the broker-readable `hello`, never in a sealed `p`.
* ECDH key => has to be fresh per connection, or the hello public key links sessions on its own. today it's per worker life.

### revocation

instant revocation of an anonymous credential would need the broker to recognize it => contradicts unlinkability. what works:

* short epochs (the ACT `Params` carry a version string; 1 issuer key per epoch). revocation latency = epoch length.
* revoke-all for a surface = rotate that surface's key now; everyone on it re-issues.
* the `grant` frame (broker -> extension) still works for the identified side.

short epochs shrink the anonymity set + make issuance timing easier to correlate. there's no setting that wins both.

### realistic anonymity set

* self-hosted broker, 1 user, 1 extension => set size 1. the credential proves nothing the broker didn't know.
* loopback tunnel => no broker in the path; nothing to hide from.
* hosted broker => set = tunnels on the same surface with the same scope set, active in the same epoch, minus whatever IP + timing give away. for a popular surface (browser tab) that could be large; for a niche app adapter it's a handful.

---

## 4. protocol sketch

only if a hosted broker happens. nothing here is built; field names are placeholders.

new scheme id for the credential, next to the seal schemes: `act-ristretto255/v0` (matches `draft-schlesinger-cfrg-act`), or `ec-act-p256/v0` if EC-ACT settles.

http, identified (this is where the grant is checked):

```
POST /v1/cred/keys                                  ->  {epoch, scheme, keys: {<scope-set-id>: <issuer pub>}}
POST /v1/cred/issue   {secret|token, epoch, scope_set, request}
                                                    ->  {response}   or 403 if the grant is gone
```

websocket, anonymous:

```
wss://broker.example/v1/tunnel?role=client          # no token, no surface, no secret in the url
```

`hello` (`dev.conduit.hello/v2`) gains:

| field | type | meaning |
|---|---|---|
| `cred` | object | `{scheme, epoch, scope_set, spend}`; `spend` = base64url ACT spend proof for 1 credit |
| `rv` | string | rendezvous id for this epoch: HKDF over a seed both ends share, info = epoch. the broker pairs ends by `rv` only |

`instance` + any long-lived id drop out of `hello` in this mode.

`welcome` (`dev.conduit.welcome/v2`) gains:

| field | type | meaning |
|---|---|---|
| `refund` | string | base64url ACT refund => the end builds its next credential with the remaining balance |
| `window` | integer | frames/s granted on this socket (200 unless the subscribe asked for less) |

errors, added:

| code | meaning |
|---|---|
| `cred_invalid` | proof failed |
| `cred_spent` | nullifier already seen this epoch |
| `cred_epoch` | epoch isn't current; fetch keys + re-issue |

the `rv` seed can't come from the broker (it issued the pairing secret, so it knows it). it has to come from the ends: e.g. derived from the first sealed session's ECDH, then stored by both. that adds state to both clients + a re-pair path if either loses it.

frames, scopes, ring, `seq` + sealing stay as they are. scope checks use the `scope_set` named in the credential instead of a grant lookup.

---

## 5. experiment

throwaway in `/tmp/act-exp`, not committed. `anonymous-credit-tokens` 0.4.2 (ristretto255), release build. issue a balance, spend 1 credit 200 times, each with a nullifier set check + refund. Apple M5 Max; native = aarch64, wasm = `wasm32-wasip1` run by node 24.15.0 `node:wasi` (V8, the engine behind a chromium service worker; not an extension worker itself).

| | L=8 (balance 200) native | L=8 wasm | L=16 (balance 12000) native | L=16 wasm |
|---|---|---|---|---|
| issue | 214 µs | 1.43 ms | 209 µs | 570 µs |
| client prove, avg of 200 | 491 µs | 963 µs | 794 µs | 1480 µs |
| broker verify + refund, avg | 586 µs | 1161 µs | 978 µs | 1863 µs |
| client finalize, avg | 68 µs | 137 µs | 72 µs | 136 µs |
| spend proof (cbor) | 1628 B | same | 2724 B | same |
| refund / issuance response / request | 176 / 211 / 141 B | same | 176 / 211 / 141 B | same |

`L` = bit width of the range proof; cost grows with it. wasm binary: 387903 B (wasi release build).

also checked: a replayed nullifier is caught by the set; after spending the full 200, the client lib refuses the next spend with `InvalidAmount`. i didn't test a hand-built over-spend against the broker side.

single run each, no warmup control => order-of-magnitude numbers. they back §3: fine per connection, wrong per frame.

---

## 6. browser + MV3 feasibility

* WebCrypto covers ECDH/ECDSA on P-256/P-384/P-521, X25519/Ed25519 (Web Cryptography Level 2), RSA, AES, HKDF. it has no raw point add or scalar mult on arbitrary points, no hash-to-curve (RFC 9380), no ristretto255, no blind-RSA blinding. Cloudflare's `voprf-ts` says it plainly: "prime field and elliptic curve operations are not covered by the standard API", and plugs in sjcl or `@noble/curves`.
* => P-256 schemes (ARC, EC-ACT) get no help from WebCrypto either. the curve matching the existing seal doesn't make the math native.
* MV3 => Chrome's minimum extension-pages policy is already `script-src 'self' 'wasm-unsafe-eval'; object-src 'self';` => shipping a rust crate as wasm inside the package is allowed; no remote code.
* safari => *unverified*: whether `wasm-unsafe-eval` behaves the same in safari web extensions. the safari native host could run the rust directly instead, which sidesteps it.
* blind RSA in js => `blindrsa-ts` exists; per `privacypass-ts`, partially-blind (metadata) verification doesn't work in browsers.

---

## 7. where this doesn't help

* the broker sees both IPs + both sockets for the whole session. a credential hides the grant id, not the network.
* the broker pairs 1 extension with 1 client. it needs some rendezvous key; anything stable links sessions, anything rotating breaks resume at rollover.
* timing: issuance happens on an identified request; if the next anonymous `hello` follows it by 200 ms, they're linked. with 1 user per broker, timing alone is enough.
* traffic shape: frame sizes + rates per socket are visible. sealing hides content, not volume.
* self-hosted or loopback => there's no one to hide from.
* the app + the client themselves know who they are; this only concerns the broker.
* audit: "the user has to be able to ... audit what crossed it" (`docs/architecture.md`). an anonymous broker can't attribute traffic to a grant => the audit has to move to the ends.
* revocation gets slower (epoch-bound) for a privacy gain that's 0 at set size 1.

cheaper wins that don't need any of this, noted for later: tokens + secrets out of the query string (proxies + access logs record urls); a fresh ECDH key per connection; implement the 200 frames/s limit that the spec already promises.

---

## 8. open questions

1. who runs the broker in the first release? if the answer is "the user" or "loopback", stop here.
2. is the threat the broker operator, or someone who later gets the broker's logs? the second is mostly solved by logging less.
3. EC-ACT (P-256) vs ACT (ristretto255): pick when MOLE becomes a WG or doesn't. neither gets native WebCrypto support.
4. post-quantum: the WG killed ARC over PQ unlinkability. ACT + EC-ACT are DLOG-based too. *unverified* what MOLE plans here.
5. epoch length vs revocation latency vs set size: needs real usage numbers, which don't exist yet.
6. where does the `rv` seed live, and what happens when 1 end loses it?
7. can an extension service worker call Chrome's private state tokens api at all? not checked.

---

## 9. sources

* RFC 9576, The Privacy Pass Architecture, June 2024 => https://www.rfc-editor.org/rfc/rfc9576
* RFC 9577, The Privacy Pass HTTP Authentication Scheme, June 2024 => https://www.rfc-editor.org/rfc/rfc9577
* RFC 9578, Privacy Pass Issuance Protocols, June 2024 (token types `0x0001`, `0x0002`) => https://www.rfc-editor.org/rfc/rfc9578
* RFC 9497, OPRFs using prime-order groups, December 2023 => https://www.rfc-editor.org/rfc/rfc9497
* RFC 9474, RSA Blind Signatures, October 2023 => https://www.rfc-editor.org/rfc/rfc9474
* RFC 9496, ristretto255 and decaf448, December 2023 => https://www.rfc-editor.org/rfc/rfc9496
* RFC 9380, Hashing to Elliptic Curves, August 2023 => https://www.rfc-editor.org/rfc/rfc9380
* ARC crypto draft + its history (dead 2026-07-22, expired 2026-09-03) => https://datatracker.ietf.org/doc/draft-ietf-privacypass-arc-crypto/
* ARC protocol draft => https://datatracker.ietf.org/doc/draft-ietf-privacypass-arc-protocol/
* ACT crypto draft => https://datatracker.ietf.org/doc/draft-schlesinger-cfrg-act/
* ACT Privacy Pass draft => https://datatracker.ietf.org/doc/draft-schlesinger-privacypass-act/
* EC-ACT draft => https://datatracker.ietf.org/doc/draft-schlesinger-mole-ec-act/
* MOLE architecture draft => https://datatracker.ietf.org/doc/draft-jms-mole-architecture/
* MOLE group page => https://datatracker.ietf.org/wg/mole/about/
* rate-limited token issuance draft => https://datatracker.ietf.org/doc/draft-ietf-privacypass-rate-limit-tokens/
* batched token issuance draft => https://datatracker.ietf.org/doc/draft-ietf-privacypass-batched-tokens/
* BBS signatures draft => https://datatracker.ietf.org/doc/draft-irtf-cfrg-bbs-signatures/
* Cloudflare, "Anonymous credentials: rate-limiting bots and agents without compromising privacy", 2025-10-30 => https://blog.cloudflare.com/private-rate-limiting/
* Cloudflare, private access tokens => https://blog.cloudflare.com/eliminating-captchas-on-iphones-and-macs-using-new-standard/
* Apple, WWDC22 "Replace CAPTCHAs with Private Access Tokens" => https://developer.apple.com/videos/play/wwdc2022/10077/
* Google, Privacy Sandbox update => https://privacysandbox.google.com/blog/update-on-plans-for-privacy-sandbox-technologies
* Google, private state tokens => https://privacysandbox.google.com/protections/private-state-tokens
* Chase, Perrin, Zaverucha, The Signal Private Group System ... => https://eprint.iacr.org/2019/1416
* libsignal zkcredential => https://github.com/signalapp/libsignal/tree/main/rust/zkcredential
* RLN docs => https://rate-limiting-nullifier.github.io/rln-docs/
* zerokit (`rln` crate) => https://github.com/vacp2p/zerokit
* `anonymous-credit-tokens` => https://github.com/SamuelSchlesinger/anonymous-credit-tokens
* `privacypass` (rust) => https://github.com/raphaelrobert/privacypass
* `privacypass-ts` => https://github.com/cloudflare/privacypass-ts
* `voprf-ts` => https://github.com/cloudflare/voprf-ts
* `blindrsa-ts` => https://github.com/cloudflare/blindrsa-ts
* ARC reference impl => https://github.com/ietf-wg-privacypass/draft-arc
* Chrome extension CSP => https://developer.chrome.com/docs/extensions/reference/manifest/content-security-policy
* Web Cryptography Level 2 => https://www.w3.org/TR/webcrypto/
* crate versions => crates.io api, 2026-11-19
