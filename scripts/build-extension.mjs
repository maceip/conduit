/*
 * Put an extension together: the browser's own files (manifest, popup)
 * + the shared background, sealing and surface. the manifest names
 * background.js at its root => the shared files have to sit next to it.
 *
 *   node scripts/build-extension.mjs chromium   # => target/extension/chromium-mv3
 *   node scripts/build-extension.mjs safari     # => target/extension/safari-mv3
 *
 * load the output dir unpacked.
 */

import { cp, mkdir, readdir, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("..", import.meta.url));

export async function buildExtension(browser, out = join(ROOT, "target", "extension")) {
  if (browser !== "chromium" && browser !== "safari") {
    throw new Error(`no such browser: ${browser} (chromium or safari)`);
  }
  const name = `${browser}-mv3`;
  const source = join(ROOT, "clients", name);
  const shared = join(ROOT, "clients", "shared");
  const target = join(out, name);
  await rm(target, { recursive: true, force: true });
  await mkdir(target, { recursive: true });
  await cp(source, target, { recursive: true });
  for (const file of await readdir(shared)) {
    if (file.endsWith(".js") && !file.endsWith(".test.js")) {
      await cp(join(shared, file), join(target, file));
    }
  }
  return target;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const browsers = process.argv[2] ? [process.argv[2]] : ["chromium", "safari"];
  for (const browser of browsers) {
    console.log(await buildExtension(browser));
  }
}
