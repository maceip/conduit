# Conduit build plan

**What has to happen to finish this, and what "finished" is allowed to
mean.**  First written 2026-11-18, after the health check below.

This plan exists because the research and the first implementation landed
in one day and the remaining work is now well defined.  It is a plan, not
a claim: nothing here counts as done until its gate is green.

---

## 1. The health check this plan stands on

Run on 2026-11-18 on macOS (arm64), at the commit this plan is filed
against:

| gate | command | result |
|---|---|---|
| the protocol, broker and client | `cargo test --workspace` | **31 passed, 0 failed** |
| the surfaces, in node | `node --test clients/shared/sealing.test.js clients/shared/surface-browser.test.js` | **12 passed, 0 failed** |
| the sealing, in a real browser | `node scripts/browser-test.mjs` | **5 passed, 0 failed** — the browser's ciphertext is the one the Rust seals, byte for byte |

The repository had no `package.json`, so the browser test could not run
at all on a fresh clone.  It now has a minimal one: playwright is a
devDependency, and the two node commands above are its scripts.  That is
a health fix, not a feature.

**Every phase below lands only while these three gates stay green.**  A
phase that turns one red is not landed.

---

## 2. What "finished" means here

Five things, in this order.  The first two are the finish line the
research pointed at; the last three are the generic work that later
projects will stand on.

1. **The extension, verified in a browser, end to end.**  The README says
   it plainly: no one has loaded the extension into Chrome and watched a
   tunnel open.  Until that happens the claim is only that the parts
   work.  *Gate:* a scripted run that loads `clients/chromium-mv3` into a
   real Chromium, starts `conduit-broker`, and shows a far end receiving
   a sealed frame from the tab surface.

2. **The research gaps closed.**  `docs/research/extension-surfaces.md` §6
   lists the surfaces known but **not checked** — Audio Units, Raycast
   extensions, Home Assistant, the Docker socket, the Chrome DevTools
   Protocol, and the rest.  Each stays marked *not checked* until it has
   a source.  *Gate:* every row in §6 and every *not checked* row in the
   catalogue either carries a source or is deleted.

3. **The near end becomes a plug, not a copy.**  Today the tunnel loop,
   the ping, the backoff, the resume and the sealing live inside
   `clients/shared/background.js`, wired to one hard-coded surface.  The
   next surface would copy all of it.  *Gate:* a surface is a module that
   declares `schema`, `channel`, `scopes` and `read()`, and the near end
   loads any number of them; the tab surface is the first registrant and
   a second real surface is the proof.

4. **A second surface that is real.**  Not a stub and not a mock: a
   surface whose data comes from an API that actually exists on the near
   end.  The browser's own window list is the first candidate — the same
   tabs API the tab surface already uses, a different channel and scope.
   *Gate:* its test reads a real window list from a real browser API
   double that behaves like the one in Chrome and Safari, and the
   catalogue gains its row.

5. **The Mail adapter, started for real.**  `dev.conduit.mail.compose/v1`
   is already the broker's bootstrap surface and has never had an adapter
   behind it.  A MailKit extension is a separate Xcode target and cannot
   be tested here, so what lands is the adapter *contract* in Rust —
   the trait a Mail or desktop adapter implements, with the tab adapter's
   behaviour as the worked example and the mail shape from the research
   (compose, action, content blocker, message security) as the declared
   scopes.  *Gate:* the trait is exercised by tests against the example;
   nothing claims to read Mail that does not read Mail.

---

## 3. The boundary: what this is not

**Conduit is a tunnel, not a plugin runtime.**  It does not load
third-party code into a host process, it does not define an ABI, and it
does not run untrusted modules in a sandboxed VM.  Those are the shapes
of Extism and of every "universal plugin system" before it, and copying
them would put Conduit in the same list the catalogue is arguing against:
another in-process plugin surface, when the gap the research found is the
tunnel *between* apps and clients that none of those ship.

What is borrowed is small and named in the research: MCP's reverse-DNS
schema ids and `ui://`-style schemes, Chrome's MV3 worker lifetime rules,
Apple's extension-point model for the near end.  What is refused is
MCP's JSON-RPC transaction shape (wrong for a worker killed every thirty
seconds) and any plugin-host architecture.

Concretely: no WebAssembly runtime, no host ABI, no module loader for
third-party code, no marketplace.  The extension is the user's, installed
through the app's own surface; Conduit carries its frames.

---

## 4. The phases

| phase | what lands | gate |
|---|---|---|
| **P0 — health** (done) | `package.json`, playwright as a devDependency, the three commands above recorded and green | the table in §1 |
| **P1 — the browser run** | an end-to-end script: broker + chromium-mv3 loaded + a far end; one sealed frame observed | `cargo test --workspace` green; the e2e script exits 0 and prints the received frame |
| **P2 — the research gaps** | every §6 and *not checked* row sourced or deleted | no `not checked` left in either research document |
| **P3 — the surface plug** | `clients/shared/near-end.js`, surfaces as modules, the tab surface moved onto it | node suite green; a second surface registered and tested |
| **P4 — the second surface** | the window surface (`dev.conduit.browser.windows/v1`), its scopes, its catalogue row | its tests green in the same run as the tab surface |
| **P5 — the adapter contract** | a `SurfaceAdapter` trait in Rust with a worked example; the mail scopes declared from the research | its tests green under `cargo test --workspace` |

P1 and P2 are independent and may land in either order.  P3 comes before
P4 because the second surface must not copy the first one's plumbing.
P5 is last because the trait is shaped by what P3–P4 teach.

---

## 5. The rules the phases follow

* **No mock data anywhere.**  A surface that cannot read real state
  publishes nothing; it does not publish a placeholder.  A row without a
  source says *not checked*; it is not cited.
* **The three gates run before every commit.**  A commit that cannot run
  them says so in its message.
* **The near end may not grow a second copy of the tunnel loop.**  If a
  new surface needs plumbing, the plumbing moves into the shared module
  first.
* **Claims match evidence.**  "Verified in a browser" means a script
  loaded the extension and observed a frame; "works" means its tests
  pass; the README's honesty table is updated in the same commit as the
  change that moves a row.

---

## 6. Not in this plan

The hosted broker, the device flow's UX in a real browser, the Safari
extension run on a real Mac (P1 covers Chromium only; Safari's native
host is already tested at the socket level), and every adapter after the
first one.  Those are later work and they stand on what this plan lands.
