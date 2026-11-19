// The harness of a surface check.
//
// Every surface is checked the same way: a real broker is started, a
// real near end is started with that surface's declaration, the
// plugin's own code publishes something, and the client at the far end
// must receive it, opened.
//
// Nothing here is a mock.  The broker is the broker, the near end is
// the near end, and the client is the Node client that a script would
// use.

import { spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
/** The root of the repository. */
export const ROOT = resolve(HERE, "..", "..");

/**
 * The binary of the broker, built if it is not there.
 *
 * @returns {string} the path of the binary
 */
export function brokerBinary() {
  const path = join(ROOT, "target", "debug", "conduit-broker");
  if (!existsSync(path)) {
    throw new Error(
      `${path} is not there; run \`cargo build -p conduit-broker -p conduit-near-end\` first`,
    );
  }
  return path;
}

/**
 * The binary of the near end, built if it is not there.
 *
 * @returns {string} the path of the binary
 */
export function nearEndBinary() {
  const path = join(ROOT, "target", "debug", "conduit-near-end");
  if (!existsSync(path)) {
    throw new Error(
      `${path} is not there; run \`cargo build -p conduit-broker -p conduit-near-end\` first`,
    );
  }
  return path;
}

/**
 * The declaration of a surface.
 *
 * @param {string} directory the directory of the surface
 * @returns {object} the declaration
 */
export function declaration(directory) {
  const path = join(directory, "surface.json");
  if (!existsSync(path)) {
    throw new Error(`${path} is not there`);
  }
  return JSON.parse(readFileSync(path, "utf8"));
}

/**
 * Run something with a live broker, a live near end and a live client.
 *
 * @param {object} surface the declaration of the surface
 * @param {(context: object) => Promise<void>} body what the check does
 * @returns {Promise<void>}
 */
export async function withSurface(surface, body) {
  const directory = mkdtempSync(join(tmpdir(), "conduit-surface-"));
  const socket = join(directory, "near-end.sock");

  // The bootstrap of the broker: the surface, the grant and the token.
  const bootstrap = join(directory, "bootstrap.json");
  writeFileSync(
    bootstrap,
    JSON.stringify({
      token: "bootstrap-token",
      surfaces: [
        {
          id: surface.id,
          schemas: surface.schemas || [],
          scopes: surface.scopes || [],
        },
      ],
      grants: [
        {
          surface: surface.id,
          instance: "bootstrap",
          scopes: surface.scopes || [],
        },
      ],
    }),
  );

  // The near end's configuration.
  const configuration = join(directory, "near-end.json");
  writeFileSync(
    configuration,
    JSON.stringify({
      broker: "PLACEHOLDER",
      instance: "bootstrap",
      surfaces: [
        {
          id: surface.id,
          channel: surface.channel || surface.id,
          schemas: surface.schemas || [],
          scopes: surface.scopes || [],
          socket,
          tcp: surface.tcp,
          secret: "bootstrap-secret",
        },
      ],
    }),
  );

  // The broker.
  const broker = spawn(brokerBinary(), [], {
    env: {
      ...process.env,
      CONDUIT_LISTEN: "127.0.0.1:0",
      CONDUIT_BOOTSTRAP: bootstrap,
      CONDUIT_CLIENT_TOKEN: "bootstrap-token",
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const address = await readAddress(broker);
  const brokerErrors = [];
  broker.stderr.on("data", (chunk) => brokerErrors.push(chunk.toString()));

  // The near end, with the address of the broker in it.
  const configurationText = readFileSync(configuration, "utf8").replace(
    "PLACEHOLDER",
    `${address}/v1/tunnel`,
  );
  writeFileSync(configuration, configurationText);
  const nearEnd = spawn(nearEndBinary(), [configuration], {
    stdio: ["ignore", "pipe", "pipe"],
  });
  const nearEndErrors = [];
  nearEnd.stderr.on("data", (chunk) => nearEndErrors.push(chunk.toString()));

  try {
    // Wait for the socket of the near end.
    const deadline = Date.now() + 15000;
    while (!existsSync(socket)) {
      if (Date.now() > deadline) {
        throw new Error(
          `the near end did not bind its socket; ${nearEndErrors.join("")}`,
        );
      }
      await new Promise((resolve) => setTimeout(resolve, 50));
    }

    // Wait until the near end says the tunnel is up: a client that
    // arrives before the extension is refused by the broker.
    const { connect } = await import("../../clients/node/client.mjs");
    const { connect: connectNearEnd } = await import("../../clients/native/near-end.js");
    const probe = connectNearEnd(socket);
    await waitForTheTunnel(probe);

    const client = await connect({
      broker: `${address}/v1/tunnel`,
      surface: surface.id,
      channel: surface.channel || surface.id,
      token: "bootstrap-token",
      scopes: surface.scopes || [],
    });

    try {
      await body({
        address,
        socket,
        client,
        surface,
        directory,
        brokerErrors,
        nearEndErrors,
      });
    } catch (error) {
      // What the broker and the near end said is part of the failure.
      const noise = [...brokerErrors, ...nearEndErrors].join("").trim();
      if (noise) {
        error.message = `${error.message}\n--- the broker and the near end said ---\n${noise}`;
      }
      throw error;
    }
    client.close();
  } finally {
    nearEnd.kill("SIGKILL");
    broker.kill("SIGKILL");
  }
}

/**
 * Run something with a live broker and a near end that speaks over the
 * standard input and output.
 *
 * This is what a browser does with a native messaging host: it starts
 * the process, writes to its standard input and reads what comes out
 * of its standard output.  The near end is told where the broker is
 * by the `connect` message.
 *
 * @param {object} surface the declaration of the surface
 * @param {(context: object) => Promise<void>} body what the check does
 * @returns {Promise<void>}
 */
export async function withNativeSurface(surface, body) {
  const directory = mkdtempSync(join(tmpdir(), "conduit-native-surface-"));

  const bootstrap = join(directory, "bootstrap.json");
  writeFileSync(
    bootstrap,
    JSON.stringify({
      token: "bootstrap-token",
      surfaces: [
        {
          id: surface.id,
          schemas: surface.schemas || [],
          scopes: surface.scopes || [],
        },
      ],
      grants: [
        {
          surface: surface.id,
          instance: "bootstrap",
          scopes: surface.scopes || [],
        },
      ],
    }),
  );

  // The near end's configuration: the browser says where the broker
  // is, so nothing is said here.
  const configuration = join(directory, "near-end.json");
  writeFileSync(
    configuration,
    JSON.stringify({
      broker: "",
      instance: "bootstrap",
      surfaces: [
        {
          id: surface.id,
          channel: surface.channel || surface.id,
          schemas: surface.schemas || [],
          scopes: surface.scopes || [],
          socket: join(directory, "unused.sock"),
          secret: "bootstrap-secret",
        },
      ],
    }),
  );

  const broker = spawn(brokerBinary(), [], {
    env: {
      ...process.env,
      CONDUIT_LISTEN: "127.0.0.1:0",
      CONDUIT_BOOTSTRAP: bootstrap,
      CONDUIT_CLIENT_TOKEN: "bootstrap-token",
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const address = await readAddress(broker);
  const brokerErrors = [];
  broker.stderr.on("data", (chunk) => brokerErrors.push(chunk.toString()));

  const nearEnd = spawn(nearEndBinary(), ["--native", configuration], {
    stdio: ["pipe", "pipe", "pipe"],
  });
  nearEnd.stderr.on("data", (chunk) => {
    process.stderr.write(`[near end] ${chunk}`);
  });
  const nearEndErrors = [];
  nearEnd.stderr.on("data", (chunk) => {
    const text = chunk.toString();
    nearEndErrors.push(text);
    if (process.env.CONDUIT_CHECK_VERBOSE) {
      process.stderr.write(`[near end] ${text}`);
    }
  });

  try {
    const native = nativePort(nearEnd);

    // Wait until the near end says the tunnel is up: a client that
    // arrives before the extension is refused by the broker.  The
    // connect frame is what says where the broker is, and the handler
    // is in place before it goes out.
    await waitForTheTunnel(native, () => {
      native.send({
        t: "connect",
        s: surface.id,
        c: surface.channel || surface.id,
        seq: 0,
        p: {
          broker: `${address}/v1/tunnel`,
          secret: "bootstrap-secret",
          instance: "bootstrap",
          surface: surface.id,
          channel: surface.channel || surface.id,
        },
      });
    });

    const { connect } = await import("../../clients/node/client.mjs");
    const client = await connect({
      broker: `${address}/v1/tunnel`,
      surface: surface.id,
      channel: surface.channel || surface.id,
      token: "bootstrap-token",
      scopes: surface.scopes || [],
    });

    try {
      await body({
        address,
        native,
        client,
        surface,
        directory,
        brokerErrors,
        nearEndErrors,
      });
    } catch (error) {
      const noise = [...brokerErrors, ...nearEndErrors].join("").trim();
      if (noise) {
        error.message = `${error.message}\n--- the broker and the near end said ---\n${noise}`;
      }
      throw error;
    }
    client.close();
  } finally {
    nearEnd.kill("SIGKILL");
    broker.kill("SIGKILL");
  }
}

/**
 * The near end's end of the native messaging protocol.
 *
 * @param {object} child the near end process
 * @returns {object} the port: send a message, listen for them
 */
function nativePort(child) {
  const handlers = [];
  let buffer = Buffer.alloc(0);

  child.stdout.on("data", (chunk) => {
    buffer = Buffer.concat([buffer, chunk]);
    for (;;) {
      if (buffer.length < 4) {
        return;
      }
      const length = buffer.readUInt32BE(0);
      if (buffer.length < 4 + length) {
        return;
      }
      const body = buffer.subarray(4, 4 + length).toString("utf8");
      buffer = buffer.subarray(4 + length);
      let message;
      try {
        message = JSON.parse(body);
      } catch {
        continue;
      }
      if (process.env.CONDUIT_CHECK_VERBOSE) {
        console.error("native host said:", JSON.stringify(message));
      }
      for (const handler of handlers) {
        handler(message);
      }
    }
  });

  return {
    send(message) {
      const body = Buffer.from(JSON.stringify(message), "utf8");
      const length = Buffer.alloc(4);
      length.writeUInt32BE(body.length, 0);
      child.stdin.write(Buffer.concat([length, body]));
    },
    on(handler) {
      handlers.push(handler);
    },
    /** Wait for a frame of that kind. */
    async next(kind, timeoutMs = 10000) {
      const started = Date.now();
      for (;;) {
        const message = await new Promise((resolve) => {
          const timer = setTimeout(() => resolve(null), 100);
          handlers.push((candidate) => {
            if (candidate.t === kind) {
              clearTimeout(timer);
              resolve(candidate);
            }
          });
        });
        if (message) {
          return message;
        }
        if (Date.now() - started > timeoutMs) {
          throw new Error(`no ${kind} arrived in ${timeoutMs}ms`);
        }
      }
    },
  };
}

/**
 * Wait until the near end says the tunnel is up.
 *
 * The near end tells the plugins when the broker has welcomed it: the
 * welcome frame is the tunnel coming up.  There is nothing to ask and
 * nothing to wait for on the plugin's side — a plugin publishes when it
 * has something to say, and the near end holds it until there is a key
 * to seal it under.
 *
 * @param {object} port the near end's end: a socket or a pipe
 * @returns {Promise<void>}
 */
async function waitForTheTunnel(port, kick) {
  const started = Date.now();
  const connected = new Promise((resolve) => {
    const handler = (frame) => {
      if (frame && frame.t === "welcome") {
        resolve();
      }
    };
    if (port.on) {
      port.on(handler);
    }
  });
  // The handler is in place before anything is said, so the welcome
  // cannot slip past.
  if (kick) {
    kick();
  }
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline) {
    const race = await Promise.race([
      connected.then(() => true),
      new Promise((resolve) => setTimeout(() => resolve(false), 100)),
    ]);
    if (race) {
      return;
    }
  }
  throw new Error("the near end did not say the tunnel was up");
}

/**
 * Read the address the broker printed.
 *
 * @param {object} broker the child process
 * @returns {Promise<string>} the address of the broker
 */
function readAddress(broker) {
  return new Promise((resolve, reject) => {
    let text = "";
    const timer = setTimeout(() => {
      reject(new Error(`the broker did not say where it was: ${text}`));
    }, 15000);
    broker.stdout.on("data", (chunk) => {
      text += chunk.toString();
      const line = text.split("\n").find((line) => line.includes("listening on"));
      if (line) {
        clearTimeout(timer);
        resolve(line.split("listening on ")[1].replace("/v1/tunnel", "").trim());
      }
    });
    broker.on("exit", (code) => {
      clearTimeout(timer);
      reject(new Error(`the broker stopped with ${code}: ${text}`));
    });
  });
}

/**
 * Say that a check passed.
 *
 * @param {string} message what passed
 */
export function pass(message) {
  console.log(`ok   ${message}`);
}

/**
 * Say that a check failed.
 *
 * @param {string} message what failed
 */
export function fail(message) {
  console.error(`FAIL ${message}`);
  process.exitCode = 1;
}

/**
 * Run a check, and say what happened.
 *
 * @param {string} name the name of the surface
 * @param {() => Promise<void>} body the check
 * @returns {Promise<void>}
 */
export async function check(name, body) {
  try {
    await body();
    pass(name);
  } catch (error) {
    fail(`${name}: ${error && error.message ? error.message : error}`);
    if (process.env.CONDUIT_CHECK_VERBOSE) {
      console.error(error);
    }
  }
  // The sockets of the check are still open, and a socket keeps the
  // process alive: the check is over, so the process is over.
  process.exit(process.exitCode || 0);
}

/** Make a directory, if it is not there. */
export function ensure(directory) {
  if (!existsSync(directory)) {
    mkdirSync(directory, { recursive: true });
  }
  return directory;
}
