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

import { cp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
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

/**
 * Put the Figma plugin together.
 *
 * A Figma plugin has to be self-contained: the sandbox serves the
 * plugin's own files and nothing else.  So the near end's web client
 * is inlined into the iframe's script, and the plugin's own files are
 * copied next to it.
 *
 * @param {string} [out] where the plugin is put
 * @returns {Promise<string>} the directory of the plugin
 */
export async function buildFigma(out = join(ROOT, "target", "extension")) {
  const source = join(ROOT, "surfaces", "figma");
  const target = join(out, "figma");
  await rm(target, { recursive: true, force: true });
  await mkdir(target, { recursive: true });
  await cp(source, target, { recursive: true });

  const client = await readFile(
    join(ROOT, "clients", "shared", "near-end-web.js"),
    "utf8",
  );
  const ui = await readFile(join(target, "ui.js"), "utf8");
  // The client is in the file now, so the import of it goes.
  const withoutTheImport = ui.replace(
    /^import .*near-end-web\.js";\n/m,
    "",
  );
  await writeFile(join(target, "ui.js"), `${client}\n${withoutTheImport}`);
  return target;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const wanted = process.argv[2];
  if (wanted === "figma") {
    console.log(await buildFigma());
  } else {
    const browsers = wanted ? [wanted] : ["chromium", "safari"];
    for (const browser of browsers) {
      console.log(await buildExtension(browser));
    }
  }
}
