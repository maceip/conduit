/*
 * the whole system, end to end, nothing mocked:
 *
 *   real broker (target/debug/conduit-broker) on a free port
 *   real chromium (playwright's) with the real extension loaded unpacked
 *   real rust client (crates/conduit-client/examples/e2e.rs)
 *
 * playwright drives what a user does: set the broker in the popup, approve
 * the code on the broker's own page, browse, press stop + start. the
 * extension's websocket is watched over the devtools protocol, so the
 * frames on the wire are checked as sent, not as the extension claims.
 *
 *   node scripts/e2e/run.mjs           # headless (new headless runs extensions)
 *   E2E_HEADED=1 node scripts/e2e/run.mjs
 *
 * exit 0 => every check passed. logs => target/e2e/
 */

import { spawn, spawnSync } from "node:child_process";
import { createServer } from "node:http";
import { createServer as createTcpServer } from "node:net";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { createWriteStream } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { randomBytes } from "node:crypto";
import { chromium } from "playwright";
import { buildExtension } from "../build-extension.mjs";

const ROOT = fileURLToPath(new URL("../..", import.meta.url));
const LOGS = join(ROOT, "target", "e2e");
const SURFACE = "dev.conduit.browser.tabs/v1";
const ADDRESS = "conduit://dev.conduit.browser.tabs/tabs@v1";
const WINDOWS_SURFACE = "dev.conduit.browser.windows/v1";
const WINDOWS_ADDRESS = "conduit://dev.conduit.browser.windows/windows@v1";
const HEADED = process.env.E2E_HEADED === "1";
const FLOOD = 1000;

const results = [];
const cleanup = [];
let logIndex = 0;

function check(name, ok, detail = "") {
  results.push({ name, ok: !!ok, detail });
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail ? ` => ${detail}` : ""}`);
  return !!ok;
}

function log(...parts) {
  console.log("    ", ...parts);
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/* Poll until `probe` returns something truthy, or time out with null. */
async function until(probe, { timeout = 15000, every = 100 } = {}) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    const value = await probe();
    if (value) {
      return value;
    }
    await sleep(every);
  }
  return null;
}

function freePort() {
  return new Promise((resolve, reject) => {
    const server = createTcpServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });
}

function fromBase64Url(text) {
  return Buffer.from(text.replace(/-/g, "+").replace(/_/g, "/"), "base64");
}

const BASE64URL = /^[A-Za-z0-9_-]+$/;

/* ---- build ---- */

function build() {
  for (const args of [
    ["build", "-p", "conduit-broker", "--bin", "conduit-broker"],
    ["build", "-p", "conduit-client", "--example", "e2e"],
  ]) {
    console.log(`$ cargo ${args.join(" ")}`);
    const run = spawnSync("cargo", args, { cwd: ROOT, stdio: "inherit" });
    if (run.status !== 0) {
      throw new Error(`cargo ${args.join(" ")} failed`);
    }
  }
}

/* ---- the broker ---- */

async function startBroker(token) {
  const out = createWriteStream(join(LOGS, "broker.log"));
  const broker = spawn(join(ROOT, "target", "debug", "conduit-broker"), [], {
    cwd: ROOT,
    env: { ...process.env, CONDUIT_LISTEN: "127.0.0.1:0", CONDUIT_CLIENT_TOKEN: token },
    stdio: ["ignore", "pipe", "pipe"],
  });
  cleanup.push(() => broker.kill("SIGTERM"));
  broker.stderr.pipe(out);
  const url = await new Promise((resolve, reject) => {
    let buffered = "";
    const timer = setTimeout(() => reject(new Error("the broker never said where it listens")), 15000);
    broker.stdout.on("data", (chunk) => {
      out.write(chunk);
      buffered += chunk.toString();
      const match = buffered.match(/listening on (ws:\/\/\S+)/);
      if (match) {
        clearTimeout(timer);
        resolve(match[1]);
      }
    });
    broker.once("exit", (code) => reject(new Error(`the broker exited with ${code}`)));
  });
  return { broker, url, http: url.replace(/^ws/, "http").replace(/\/v1\/tunnel$/, "") };
}

/* ---- pages to browse: real http, real titles ---- */

function startPages() {
  const server = createServer((request, response) => {
    const id = decodeURIComponent((request.url || "/").split("/").pop() || "index");
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(`<!doctype html><title>conduit e2e ${id}</title><h1>${id}</h1>`);
  });
  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => {
      cleanup.push(() => server.close());
      resolve(`http://127.0.0.1:${server.address().port}`);
    });
  });
}

/* ---- the client ---- */

function startClient(name, broker, token, args, address = ADDRESS) {
  const out = createWriteStream(join(LOGS, `client-${++logIndex}-${name}.log`));
  const child = spawn(
    join(ROOT, "target", "debug", "examples", "e2e"),
    ["--broker", broker, "--token", token, "--address", address, ...args],
    { cwd: ROOT, stdio: ["ignore", "pipe", "pipe"] },
  );
  const client = { name, child, lines: [], exited: null };
  let buffered = "";
  child.stdout.on("data", (chunk) => {
    out.write(chunk);
    buffered += chunk.toString();
    let newline;
    while ((newline = buffered.indexOf("\n")) >= 0) {
      const text = buffered.slice(0, newline).trim();
      buffered = buffered.slice(newline + 1);
      if (text) {
        try {
          client.lines.push(JSON.parse(text));
        } catch (error) {
          client.lines.push({ type: "unparsed", text });
        }
      }
    }
  });
  child.stderr.pipe(out);
  child.once("exit", (code, signal) => {
    client.exited = { code, signal };
  });
  cleanup.push(() => child.exitCode === null && child.kill("SIGTERM"));
  client.find = (predicate) => client.lines.find(predicate);
  client.wait = (predicate, timeout = 15000) => until(() => client.find(predicate), { timeout });
  client.stop = async () => {
    if (client.exited) {
      return;
    }
    child.kill("SIGTERM");
    await until(() => client.exited, { timeout: 5000 });
  };
  return client;
}

/* A client that has the tunnel, retrying while the previous one is
 * still letting go of it. */
async function attachClient(name, broker, token, args, address = ADDRESS) {
  for (let attempt = 0; attempt < 10; attempt += 1) {
    const client = startClient(name, broker, token, args, address);
    const first = await client.wait((line) => line.type === "event" || line.type === "refused" || line.type === "fatal" || line.type === "closed", 10000);
    if (first && first.type === "refused" && /another client/.test(first.message || "")) {
      await client.stop();
      await sleep(300);
      continue;
    }
    return client;
  }
  throw new Error(`client ${name} never got the tunnel`);
}

/* ---- the wire: the extension's websocket, over devtools ---- */

async function tapTheWire(port, extensionId) {
  const target = await until(async () => {
    const list = await fetch(`http://127.0.0.1:${port}/json/list`).then((reply) => reply.json());
    return list.find((entry) => entry.type === "service_worker" && entry.url.startsWith(`chrome-extension://${extensionId}/`));
  });
  if (!target) {
    throw new Error("no devtools target for the extension's service worker");
  }
  const socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.onopen = resolve;
    socket.onerror = () => reject(new Error("devtools socket failed"));
  });
  const wire = { sent: [], received: [], created: [], closed: [] };
  const out = createWriteStream(join(LOGS, "wire.log"));
  let id = 0;
  const pending = new Map();
  socket.onmessage = (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      pending.get(message.id)(message);
      pending.delete(message.id);
      return;
    }
    const params = message.params || {};
    switch (message.method) {
      case "Network.webSocketCreated":
        wire.created.push({ url: params.url, at: Date.now() });
        out.write(`created ${params.url.replace(/secret=[^&]+/, "secret=…")}\n`);
        break;
      case "Network.webSocketClosed":
        wire.closed.push({ at: Date.now() });
        out.write("closed\n");
        break;
      case "Network.webSocketFrameSent":
      case "Network.webSocketFrameReceived": {
        const text = params.response && params.response.payloadData;
        let frame = null;
        try {
          frame = JSON.parse(text);
        } catch (error) {
          frame = null;
        }
        const entry = { frame, text, at: Date.now() };
        if (message.method.endsWith("Sent")) {
          wire.sent.push(entry);
          out.write(`=> ${text}\n`);
        } else {
          wire.received.push(entry);
          out.write(`<= ${text}\n`);
        }
        break;
      }
      default:
        break;
    }
  };
  const send = (method, params = {}) =>
    new Promise((resolve) => {
      id += 1;
      pending.set(id, resolve);
      socket.send(JSON.stringify({ id, method, params }));
    });
  const enabled = await send("Network.enable");
  if (enabled.error) {
    throw new Error(`Network.enable: ${enabled.error.message}`);
  }
  cleanup.push(() => socket.close());
  return wire;
}

/* ---- the run ---- */

async function main() {
  await mkdir(LOGS, { recursive: true });
  build();

  const token = randomBytes(16).toString("hex");
  const { url: broker, http } = await startBroker(token);
  console.log(`broker => ${broker}`);
  const pages = await startPages();

  const scratch = await mkdtemp(join(tmpdir(), "conduit-e2e-"));
  cleanup.push(() => rm(scratch, { recursive: true, force: true }));
  const extensionDir = await buildExtension("chromium", join(scratch, "extension"));
  const cdpPort = await freePort();

  const context = await chromium.launchPersistentContext(join(scratch, "profile"), {
    channel: "chromium",
    headless: !HEADED,
    args: [
      `--disable-extensions-except=${extensionDir}`,
      `--load-extension=${extensionDir}`,
      `--remote-debugging-port=${cdpPort}`,
    ],
  });
  cleanup.push(() => context.close());
  context.on("weberror", (error) => log("page error:", error.error().message));

  // 1. the extension loads and its worker runs.
  let worker = context.serviceWorkers().find((sw) => sw.url().startsWith("chrome-extension://"));
  if (!worker) {
    worker = await context.waitForEvent("serviceworker", { timeout: 15000 });
  }
  const extensionId = new URL(worker.url()).host;
  check("the extension loads; its service worker runs", worker.url().endsWith("/background.js"), worker.url());
  worker.on("console", (message) => log(`worker ${message.type()}: ${message.text()}`));

  const wire = await tapTheWire(cdpPort, extensionId);

  // 2. the popup: point the extension at this broker.
  const popup = await context.newPage();
  await popup.goto(`chrome-extension://${extensionId}/popup.html`);
  const before = await until(async () => {
    const text = await popup.locator("#status").textContent();
    return text && text !== "asking the tunnel…" ? text : null;
  });
  check("the popup answers before pairing: not paired", before === "not paired", `${before}; ${await popup.locator("#error").textContent()}`);
  await popup.locator("#broker").fill(broker);
  await popup.locator("#save").click();

  // 3. pairing: the extension asks for a code per surface and opens the
  //    broker's page for each.  A secret belongs to one surface, so the
  //    codes are asked for one surface at a time.
  const approvePages = await until(() => {
    const found = context.pages().filter((page) => page.url().startsWith(`${http}/v1/device/approve-page?code=`));
    return found.length >= 2 ? found : null;
  });
  check(
    "pairing: the extension asks the broker for a code per surface and opens its approve pages",
    approvePages,
    approvePages ? `${approvePages.length} page(s)` : "none",
  );
  if (!approvePages) {
    throw new Error("no approve pages; nothing after this can run");
  }
  for (const page of approvePages) {
    await page.waitForLoadState();
    const offer = await page.locator("body").textContent();
    const which = offer.includes(SURFACE) ? SURFACE : offer.includes(WINDOWS_SURFACE) ? WINDOWS_SURFACE : null;
    const scope = which === SURFACE ? "browser.tabs.read" : "browser.windows.read";
    check(
      `the approve page names the surface + the scope asked for (${which || "no surface"})`,
      which !== null && offer.includes(scope),
      offer.trim(),
    );
    await page.getByRole("button", { name: "approve" }).click();
    await page.waitForLoadState();
    const approved = await page.locator("h1").textContent();
    check(`approve on the broker's page => approved (${which || "?"})`, approved === "approved", approved);
  }

  // 4. the extension gets its secrets and opens the tunnels.
  const connected = await until(async () => (await popup.locator("#status").textContent()) === "connected", { timeout: 20000 });
  check("the popup shows connected", connected, await popup.locator("#status").textContent());
  const stored = await worker.evaluate(() => chrome.storage.local.get(null));
  const secret = stored["conduit.secret." + SURFACE] || "";
  const windowSecret = stored["conduit.secret." + WINDOWS_SURFACE] || "";
  check(
    "the secret is 256 bits, base64url, kept in storage.local with the broker",
    BASE64URL.test(secret) && fromBase64Url(secret).length === 32 && stored["conduit.broker"] === broker,
    `${fromBase64Url(secret).length} bytes; broker ${stored["conduit.broker"]}`,
  );
  check(
    "each surface has its own secret: the broker binds a secret to one surface",
    BASE64URL.test(windowSecret) && windowSecret !== secret && fromBase64Url(windowSecret).length === 32,
    `${fromBase64Url(windowSecret).length} bytes`,
  );
  const created = wire.created.find((entry) => new URL(entry.url).searchParams.get("surface") === SURFACE);
  check(
    "the extension's socket presents the secret on the tunnel url",
    created && new URL(created.url).searchParams.get("secret") === secret && new URL(created.url).searchParams.get("surface") === SURFACE,
    created ? created.url.replace(/secret=[^&]+/, "secret=…") : "no socket",
  );
  check(
    "one socket per surface: the near end carries each one on its own",
    wire.created.filter((entry) => new URL(entry.url).searchParams.get("surface") === WINDOWS_SURFACE).length === 1,
    `${wire.created.length} socket(s) in all`,
  );

  // 5. hello + welcome, as sent and received on the wire.
  const sentHello = wire.sent.find((entry) => entry.frame && entry.frame.t === "hello");
  const welcome = wire.received.find((entry) => entry.frame && entry.frame.t === "welcome");
  check("the extension says hello; the broker welcomes it", sentHello && welcome, welcome && welcome.text);
  const theirHello = sentHello ? sentHello.frame.p || {} : {};
  const key = theirHello.crypto || {};
  check(
    "extension hello: public key = 65-byte p-256 point, prefix = 4 bytes, both base64url",
    BASE64URL.test(key.public || "") && fromBase64Url(key.public).length === 65 && fromBase64Url(key.public)[0] === 4 && BASE64URL.test(key.prefix || "") && fromBase64Url(key.prefix).length === 4,
    `public ${key.public && key.public.slice(0, 12)}…, prefix ${key.prefix}`,
  );

  // 6. a real client subscribes.
  const a = await attachClient("subscriber", broker, token, ["--scope", "browser.tabs.read"]);
  check("the client connects and subscribes", a.find((line) => line.type === "connected") && a.find((line) => line.type === "subscribe"), JSON.stringify(a.lines[0]));
  const clientHello = await until(() => wire.received.find((entry) => entry.frame && entry.frame.t === "hello" && entry.frame.p && entry.frame.p.role === "client"));
  const clientKey = clientHello ? clientHello.frame.p.crypto : {};
  check(
    "client hello reaches the extension: 65-byte key + 4-byte prefix, base64url",
    clientKey && BASE64URL.test(clientKey.public || "") && fromBase64Url(clientKey.public).length === 65 && BASE64URL.test(clientKey.prefix || "") && fromBase64Url(clientKey.prefix).length === 4,
    clientKey ? `public ${clientKey.public.slice(0, 12)}…, prefix ${clientKey.prefix}` : "none",
  );

  // 7. browse; the client opens what the extension sealed.
  const nonce = randomBytes(6).toString("hex");
  const target = await context.newPage();
  const pageUrl = `${pages}/page/${nonce}`;
  await target.goto(pageUrl);
  const title = await target.title();
  const event = await a.wait((line) => line.type === "event" && line.payload && line.payload.url === pageUrl);
  check(
    "the client opens a sealed publish to the tab playwright navigated to (url + title)",
    event && event.payload.title === title && event.schema === "dev.conduit.browser.tab/v1" && event.channel === "tabs",
    event ? JSON.stringify(event.payload) : `no event for ${pageUrl}; got ${JSON.stringify(a.lines.slice(-3))}`,
  );

  // 8. the broker never saw the plaintext.
  if (event) {
    const onTheWire = wire.sent.find((entry) => entry.frame && entry.frame.t === "publish" && entry.frame.seq === event.seq);
    const p = onTheWire ? onTheWire.frame.p : "";
    const plaintext = JSON.stringify(event.payload);
    const bytes = BASE64URL.test(p) ? fromBase64Url(p) : Buffer.alloc(0);
    check(
      "the publish p on the wire is ciphertext: not the plaintext, no url/title in it, plaintext + 16-byte tag long",
      onTheWire && p !== plaintext && !p.includes(nonce) && !bytes.includes(Buffer.from(nonce)) && !bytes.includes(Buffer.from("http")) && bytes.length === Buffer.byteLength(plaintext) + 16,
      `p ${p.slice(0, 24)}… (${bytes.length} bytes) vs plaintext ${Buffer.byteLength(plaintext)} bytes`,
    );
    check("the broker relayed p unchanged: what the extension sent is what the client got", onTheWire && event.sealed === p, `seq ${event.seq}`);
  }

  // 9. a second navigation is a second event.
  const pageUrl2 = `${pages}/page/${nonce}-2`;
  await target.goto(pageUrl2);
  const event2 = await a.wait((line) => line.type === "event" && line.payload && line.payload.url === pageUrl2);
  check("navigating again => the client gets the new tab state", event2 && event2.payload.title === `conduit e2e ${nonce}-2`, event2 ? event2.payload.title : "none");
  await a.stop();

  // 9b. the second surface: the same near end, carrying another module.
  const w = await attachClient("windows", broker, token, ["--scope", "browser.windows.read"], WINDOWS_ADDRESS);
  const windowEvent = await w.wait((line) => line.type === "event" && line.schema === "dev.conduit.browser.window/v1");
  check(
    "the second surface publishes too: the client opens a sealed window, sealed by the same near end",
    windowEvent && windowEvent.payload && windowEvent.payload.id !== null && typeof windowEvent.payload.tabs === "number",
    windowEvent ? JSON.stringify(windowEvent.payload) : JSON.stringify(w.lines.slice(-3)),
  );
  await w.stop();

  // 10. a scope outside the grant.
  const b = await attachClient("out-of-grant", broker, token, ["--scope", "browser.tabs.write"]);
  const denied = await b.wait((line) => line.type === "refused");
  check(
    "subscribe to browser.tabs.write (not granted) => scope_denied naming it",
    denied && denied.code === "scope_denied" && denied.scope === "browser.tabs.write",
    denied ? JSON.stringify(denied) : JSON.stringify(b.lines),
  );
  await b.stop();

  // 11. 200 frames/s, from a real client through the real broker.
  await target.bringToFront();
  const c = await attachClient("flood", broker, token, ["--scope", "browser.tabs.read", "--flood", String(FLOOD)]);
  const snapshot = c.find((line) => line.type === "event");
  check(
    "a new client gets the active tab on subscribe",
    snapshot && snapshot.payload.url === pageUrl2,
    snapshot ? snapshot.payload.url : JSON.stringify(c.lines),
  );
  const flood = await c.wait((line) => line.type === "flood", 20000);
  const refusedAll = flood && flood.codes && Object.keys(flood.codes).every((code) => code === "scope_denied");
  // the bucket holds 200 and refills at 200/s => over the send time t at most 200 + 200t frames
  // cross. the broker is still reading after the last send returns => t + 250 ms.
  const ceiling = flood ? 200 + Math.ceil((200 * (flood.elapsed_ms + 250)) / 1000) : 0;
  check(
    `${FLOOD} client publishes as fast as the socket takes them: answered <= 200 + 200/s x (send time + 250 ms), the rest dropped, every answer scope_denied (no write in the grant)`,
    flood && flood.refused <= ceiling && flood.refused >= 150 && flood.refused < flood.sent && refusedAll,
    flood ? `sent ${flood.sent} in ${flood.elapsed_ms} ms, answered ${flood.refused} (ceiling ${ceiling}), dropped ${flood.sent - flood.refused}, codes ${JSON.stringify(flood.codes)}` : "no flood line",
  );
  const refill = await c.wait((line) => line.type === "after_refill", 10000);
  check("a second later the socket is still open and the bucket refilled", refill && refill.answer && refill.answer.code === "scope_denied", refill ? JSON.stringify(refill.answer) : "none");

  // 12. stop from the popup.
  await popup.bringToFront();
  const socketsBefore = wire.created.length;
  const closedBefore = wire.closed.length;
  check("the popup button says stop while connected", (await popup.locator("#toggle").textContent()) === "stop");
  await popup.locator("#toggle").click();
  const stopped = await until(async () => (await popup.locator("#status").textContent()) === "stopped");
  check("press stop => the popup shows stopped", stopped, await popup.locator("#status").textContent());
  const wireClosed = await until(() => wire.closed.length > closedBefore);
  check("press stop => the extension's socket closes", wireClosed);
  const clientClosed = await c.wait((line) => line.type === "closed", 10000);
  check("press stop => the broker closes the client's end too", clientClosed);
  await sleep(1500);
  check("stopped stays stopped: no reconnect", wire.created.length === socketsBefore, `${wire.created.length - socketsBefore} new sockets`);

  // 13. start again: the stored secret, no new approval.
  const approvePageCount = context.pages().filter((page) => page.url().includes("/v1/device/approve-page")).length;
  await popup.locator("#toggle").click();
  const again = await until(async () => (await popup.locator("#status").textContent()) === "connected", { timeout: 20000 });
  check("press start => connected again", again, await popup.locator("#status").textContent());
  const newSockets = wire.created.slice(socketsBefore);
  const secretsRight = newSockets.every((entry) => {
    const url = new URL(entry.url);
    const want = url.searchParams.get("surface") === SURFACE ? secret : windowSecret;
    return url.searchParams.get("secret") === want;
  });
  check(
    "start reuses the secrets: same secret on every url, no second approve page",
    newSockets.length === 2 && secretsRight && context.pages().filter((page) => page.url().includes("/v1/device/approve-page")).length === approvePageCount,
    `${newSockets.length} new socket(s)`,
  );
  await target.bringToFront();
  const d = await attachClient("after-restart", broker, token, ["--scope", "browser.tabs.read"]);
  const afterRestart = d.find((line) => line.type === "event");
  check("after the restart a client opens the active tab again (new keys)", afterRestart && afterRestart.payload.url === pageUrl2, afterRestart ? afterRestart.payload.url : JSON.stringify(d.lines));
  await d.stop();
}

let status = 0;
try {
  await main();
} catch (error) {
  check("the run finished", false, error && error.stack ? error.stack : String(error));
}
for (const step of cleanup.reverse()) {
  try {
    await step();
  } catch (error) {
    // teardown: keep going.
  }
}
const failed = results.filter((result) => !result.ok);
console.log(`\n${results.length - failed.length} passed, ${failed.length} failed`);
await writeFile(join(LOGS, "results.json"), JSON.stringify(results, null, 2));
if (failed.length > 0 || results.length === 0) {
  status = 1;
}
process.exit(status);
