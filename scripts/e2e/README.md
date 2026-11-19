# e2e

the whole system, nothing mocked: real broker, real chromium with the real extension, real rust client.

```sh
scripts/e2e/run.sh                 # npm ci + playwright's chromium + node run.mjs
E2E_HEADED=1 scripts/e2e/run.sh    # watch it
```

exit 0 => every check passed. logs => `target/e2e/` (`broker.log`, `wire.log`, one log per client, `results.json`).

## what it runs

- `cargo build` the broker + `crates/conduit-client/examples/e2e.rs`; broker on a free port (`CONDUIT_LISTEN=127.0.0.1:0`), random client token (`CONDUIT_CLIENT_TOKEN`)
- `scripts/build-extension.mjs chromium` => loaded unpacked into playwright's chromium (`--load-extension`; branded chrome ignores it). new headless runs extensions => no xvfb
- the extension's websocket is tapped over devtools (`--remote-debugging-port`, `Network.webSocketFrame*` on the service worker) => frames are checked as sent, not as reported

## what it checks

- popup says not paired; set the broker in the popup => the extension asks for a code + opens the broker's approve page
- approve on that page => the extension gets a 256-bit secret, presents it on the tunnel, popup says connected
- hello both ways on the wire: 65-byte p-256 key + 4-byte prefix, base64url
- playwright navigates a tab => the rust client opens the sealed publish to that url + title
- `p` on the wire isn't the plaintext, has no url in it, is plaintext + 16-byte tag; the client got the same `p` the extension sent
- subscribe to `browser.tabs.write` (not granted) => `scope_denied` naming it
- 1000 client publishes as fast as the socket takes them => at most 200 + 200/s x send time answered, rest dropped, socket stays open, bucket refills after 1s
- a new client gets the active tab on subscribe
- popup stop => socket closes, broker closes the client, no reconnect; start => same secret, no new approval, a new client opens the tab again

## not covered

safari: no way to drive it here (no playwright webkit extensions, the native host needs a signed app). the safari build shares `background.js` and is untested in a browser.
