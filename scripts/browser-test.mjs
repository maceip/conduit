/*
 * The browser tests.
 *
 * The sealing has to agree with the Rust on both sides of the tunnel, so
 * one of the checks runs in a real browser: the same keys, the same
 * nonce, the same payload, the same ciphertext.  Node's WebCrypto
 * refuses a raw X25519 private key with the usage the browser wants, so
 * the vector cannot be checked there.
 *
 * Run it with: node scripts/browser-test.mjs
 */

import { createRequire } from "node:module";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";
/* Playwright is usually a dependency of the repository; in a container
 * where it is installed globally, that is where it is looked for. */
let chromium;
try {
  ({ chromium } = await import("playwright"));
} catch {
  const require = createRequire(import.meta.url);
  ({ chromium } = require("/usr/local/lib/node_modules/playwright/index.js"));
}

const ROOT = fileURLToPath(new URL("..", import.meta.url));

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
};

/* Serve the repository over http, so the browser will load the module. */
function serve() {
  const server = createServer(async (request, response) => {
    const path = normalize(decodeURIComponent(request.url.split("?")[0]));
    if (path.includes("..")) {
      response.writeHead(403).end("no");
      return;
    }
    const file = join(ROOT, path);
    try {
      const body = await readFile(file);
      response.writeHead(200, { "content-type": TYPES[extname(file)] || "application/octet-stream" });
      response.end(body);
    } catch (error) {
      response.writeHead(404).end("not found");
    }
  });
  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => resolve({ server, port: server.address().port }));
  });
}

const { server, port } = await serve();
const browser = await chromium.launch();
let status = 0;

try {
  const page = await browser.newPage();
  page.on("console", (message) => {
    if (message.type() === "error") {
      console.error("the page said:", message.text());
    }
  });
  await page.goto(`http://127.0.0.1:${port}/scripts/browser-suite.html`);
  await page.waitForFunction(() => window.__conduitDone === true, null, { timeout: 60000 });

  const results = await page.evaluate(() => window.__conduitResults);
  for (const result of results) {
    console.log(`${result.ok ? "ok  " : "FAIL"} ${result.name}${result.detail ? ` (${result.detail})` : ""}`);
    if (!result.ok) {
      status = 1;
    }
  }
  const failed = results.filter((result) => !result.ok).length;
  console.log(`${results.length - failed} passed, ${failed} failed`);
} catch (error) {
  console.error("the browser tests did not run:", error.message);
  status = 1;
} finally {
  await browser.close();
  server.close();
}

process.exit(status);
