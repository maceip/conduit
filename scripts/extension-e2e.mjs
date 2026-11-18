/*
 * The extension, in a real browser, end to end.
 *
 * The README's own unfinished claim: the parts are tested, but no one
 * has loaded the extension into a browser and watched a tunnel open.
 * This is that run.
 *
 * What it does, in order:
 *
 *   1. starts the broker on 127.0.0.1:8787;
 *   2. loads clients/chromium-mv3 into a real Chromium;
 *   3. points the extension at that broker and at the bootstrap secret
 *      (the broker's own test secret — the device flow that hands out a
 *      real secret is covered by the broker's tests);
 *   4. clicks the extension's own toggle, which is what a user does;
 *   5. runs the Rust far end, which subscribes to the tab surface and
 *      prints the first frame it opens;
 *   6. checks that the frame carries a real tab.
 *
 * Run it with: node scripts/extension-e2e.mjs
 */

import { spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import net from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

let chromium;
try {
  ({ chromium } = await import("playwright"));
} catch {
  const require = createRequire(import.meta.url);
  ({ chromium } = require("/usr/local/lib/node_modules/playwright/index.js"));
}

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const BROKER_URL = "ws://127.0.0.1:8787/v1/tunnel";
const BROKER_HTTP = "http://127.0.0.1:8787";
const SECRET = "bootstrap-secret";
const EXTENSION = join(ROOT, "clients", "chromium-mv3");

function say(line) {
  process.stdout.write(line + "\n");
}

function fail(message) {
  process.stderr.write("FAIL " + message + "\n");
  process.exit(1);
}

function run(command, args, options = {}) {
  return spawn(command, args, { cwd: ROOT, ...options });
}

/* Wait for a line of a child's stdout. */
function waitForLine(child, needle, timeoutMs) {
  return new Promise((resolve, reject) => {
    let seen = "";
    const timer = setTimeout(() => reject(new Error(`timed out waiting for ${needle}`)), timeoutMs);
    child.stdout.on("data", (chunk) => {
      seen += chunk.toString();
      if (seen.includes(needle)) {
        clearTimeout(timer);
        resolve(seen);
      }
    });
    child.on("exit", (code) => {
      clearTimeout(timer);
      reject(new Error(`${needle} never came; the process exited ${code}`));
    });
  });
}

/* The far end: one frame from the tab surface. */
function runFarEnd() {
  return new Promise((resolve, reject) => {
    const child = run("cargo", ["run", "-q", "-p", "conduit-client", "--example", "tab_frame"]);
    let out = "";
    let err = "";
    child.stdout.on("data", (chunk) => (out += chunk.toString()));
    child.stderr.on("data", (chunk) => (err += chunk.toString()));
    child.on("exit", (code) => {
      if (code === 0) {
        resolve(out.trim());
      } else {
        reject(new Error(`the far end exited ${code}: ${err}`));
      }
    });
  });
}

/* The shared near end is copied into each client dir: a browser loads
 * an extension only from inside its own directory. */
say("assembling the extension clients…");
await import("./assemble-clients.mjs");

/* 0. The port is free: one broker at a time. */
const busy = await new Promise((resolve) => {
  const socket = net.connect(8787, "127.0.0.1");
  socket.on("connect", () => {
    socket.destroy();
    resolve(true);
  });
  socket.on("error", () => resolve(false));
});
if (busy) {
  fail("port 8787 is in use; stop whatever is holding it and run this again");
}

/* 1. The broker. */
say("starting the broker…");
const broker = run("cargo", ["run", "-q", "-p", "conduit-broker"]);
let brokerLog = "";
broker.stdout.on("data", (chunk) => (brokerLog += chunk.toString()));
broker.stderr.on("data", (chunk) => (brokerLog += chunk.toString()));
await waitForLine(broker, "listening", 120_000).catch((error) => {
  broker.kill();
  fail(`the broker did not start: ${error}\n${brokerLog}`);
});
process.on("exit", () => broker.kill());

/* 2. The browser, with the extension in it. */
say("loading the extension into Chromium…");
const profile = await mkdtemp(join(tmpdir(), "conduit-e2e-"));
const context = await chromium.launchPersistentContext(profile, {
  channel: "chromium",
  headless: false,
  /* Playwright disables extensions by default; this run is the
   * exception that proves the extension works. */
  ignoreDefaultArgs: ["--disable-extensions"],
  args: [`--disable-extensions-except=${EXTENSION}`, `--load-extension=${EXTENSION}`],
});

let serviceWorker = context.serviceWorkers()[0];
if (!serviceWorker) {
  serviceWorker = await context.waitForEvent("serviceworker", { timeout: 30_000 }).catch(() => null);
}
if (!serviceWorker) {
  await context.close();
  broker.kill();
  fail("the extension's service worker never started");
}
const extensionId = new URL(serviceWorker.url()).host;
say(`the extension is ${extensionId}`);

/* 3. Point the extension at the local broker, from its own popup. */
const popup = await context.newPage();
await popup.goto(`chrome-extension://${extensionId}/popup.html`);
await popup.evaluate(
  ([brokerUrl, secret]) =>
    new Promise((resolve) => {
      const values = { "conduit.broker": brokerUrl, "conduit.secret": secret };
      if (chrome.storage && chrome.storage.session) {
        chrome.storage.session.set(values, () => {
          if (chrome.storage.local) {
            chrome.storage.local.set(values, resolve);
          } else {
            resolve(undefined);
          }
        });
      } else {
        chrome.storage.local.set(values, resolve);
      }
    }),
  [BROKER_URL, SECRET],
);

/* 4. The user's own click: the toggle in the popup. */
say("clicking the extension's toggle…");
await popup.click("#toggle");
let connected = false;
for (let attempt = 0; attempt < 30; attempt += 1) {
  const text = await popup.textContent("#status");
  if (text && text.trim() === "connected") {
    connected = true;
    break;
  }
  await popup.waitForTimeout(500);
}
if (!connected) {
  const status = await popup.textContent("#status");
  await context.close();
  broker.kill();
  fail(`the tunnel did not open; the popup says "${status}"`);
}
say("the tunnel is open");

/* 5. The far end. */
say("running the far end…");
let frame;
try {
  const printed = await runFarEnd();
  frame = JSON.parse(printed);
} catch (error) {
  await context.close();
  broker.kill();
  fail(String(error));
}

/* 6. What crossed the tunnel. */
await context.close();
broker.kill();
await rm(profile, { recursive: true, force: true });

if (typeof frame !== "object" || frame === null) {
  fail("the frame carried no object");
}
if (frame.active !== true) {
  fail(`the frame is not the active tab: ${JSON.stringify(frame)}`);
}
if (!("url" in frame) || !("title" in frame)) {
  fail(`the frame is not a tab: ${JSON.stringify(frame)}`);
}
say(`the far end opened this tab: ${JSON.stringify(frame)}`);
say("PASS the extension, the broker and the far end agree end to end");
process.exit(0);
