/*
 * Assemble the extension clients.
 *
 * The near end is the same file on both browsers, and it lives in
 * clients/shared.  A browser loads an extension only from inside the
 * extension's own directory, so the shared files are copied into each
 * client before it is loaded.  The copies are build output: they are
 * not edited, and they are not committed.
 *
 * Run it with: node scripts/assemble-clients.mjs
 */

import { copyFile, mkdir } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const SHARED = ["background.js", "near-end.js", "sealing.js", "surface-browser.js", "surface-windows.js"];
const CLIENTS = ["chromium-mv3", "safari-mv3"];

for (const client of CLIENTS) {
  const dir = join(ROOT, "clients", client);
  await mkdir(dir, { recursive: true });
  for (const file of SHARED) {
    await copyFile(join(ROOT, "clients", "shared", file), join(dir, file));
    console.log(`ok clients/${client}/${file}`);
  }
}
