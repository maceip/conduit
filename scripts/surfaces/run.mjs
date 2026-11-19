// The runner of the surface checks.
//
// Every surface that has a check is run, and the runner says what
// passed and what did not.  A surface whose check is missing is
// reported as unverified, because a surface that was never run is not
// a surface that works.
//
//   node scripts/surfaces/run.mjs            every surface
//   node scripts/surfaces/run.mjs vscode     one surface

import { existsSync, readdirSync } from "node:fs";
import { spawn } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "..", "..");
const SURFACES = join(ROOT, "surfaces");

/** The surfaces to run, or every one of them. */
const wanted = process.argv.slice(2);

/** The surfaces there are. */
function surfaces() {
  if (!existsSync(SURFACES)) {
    return [];
  }
  return readdirSync(SURFACES, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .filter((name) => wanted.length === 0 || wanted.includes(name))
    .sort();
}

/** Run one check, and say what happened. */
function run(name) {
  const directory = join(SURFACES, name);
  const script = ["check.mjs", "check.py", "check.js"].find((file) =>
    existsSync(join(directory, file)),
  );
  if (!script) {
    console.log(`skip ${name}: no check => not run => not verified`);
    return Promise.resolve(0);
  }
  const command = script.endsWith(".py") ? "python3" : "node";
  return new Promise((resolve) => {
    const child = spawn(command, [join(directory, script)], {
      stdio: "inherit",
      env: process.env,
    });
    child.on("exit", (code) => resolve(code || 0));
  });
}

let failures = 0;
for (const name of surfaces()) {
  const code = await run(name);
  if (code !== 0) {
    failures += 1;
  }
}
console.log(
  failures === 0
    ? "every surface that has a check passed"
    : `${failures} surface(s) failed`,
);
process.exit(failures === 0 ? 0 : 1);
