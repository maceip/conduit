#!/usr/bin/env node
// The MCP server of the Conduit surface.
//
// An MCP server is a plugin surface: it contributes tools and resources
// to a client that speaks the Model Context Protocol.  This one is the
// plugin of a Conduit surface: it holds the socket of the near end, it
// publishes what the app knows to the near end, and it answers the tool
// `read_surface` with the state of the surface, as text content.
//
// The transport is stdio, the way MCP says it: one JSON-RPC message per
// line, on stdin and on stdout.  Nothing else is ever written to
// stdout — the near end's chatter goes to stderr.
//
// The settings come from the environment:
//   CONDUIT_SOCKET       the socket of the near end
//   CONDUIT_CHANNEL      the channel of the surface
//   CONDUIT_STATE_FILE   a file with the state of the app, as JSON
//   CONDUIT_STATE        the state of the app, as JSON, inline

import { readFileSync } from "node:fs";
import { connect } from "../../clients/native/near-end.js";
import {
  READ_TOOL,
  STATE_RESOURCE,
  STATE_SCHEMA,
  SURFACE_URI,
  TOOL_NAME,
  eventToText,
  toolResult,
} from "./publish.js";

/** The name of this server, and its version. */
const SERVER_INFO = { name: "conduit-surface", version: "0.1.0" };

/** The protocol revisions this server speaks. */
const SUPPORTED = [
  "2024-11-05",
  "2025-03-26",
  "2025-06-18",
  "2025-11-25",
  "2026-07-28",
];

/** The revisions that are opened with an `initialize` handshake. */
const HANDSHAKE = SUPPORTED.filter((version) => version !== "2026-07-28");

const socket = process.env.CONDUIT_SOCKET || "";
const channel = process.env.CONDUIT_CHANNEL || "mcp";
const stateFile = process.env.CONDUIT_STATE_FILE || "";
const inlineState = process.env.CONDUIT_STATE || "";

/** The near end's client, if there is a near end. */
const client = socket ? connect(socket) : null;

/** The state of the surface, as the near end last saw it. */
let latest = { schema: STATE_SCHEMA, channel, payload: readState() };

/** The last status the near end said. */
let lastStatus = null;

/**
 * The state of the app, as JSON.
 *
 * A real app would read its own state; here it is a file, so that the
 * check can put a state in and see it come out.
 *
 * @returns {unknown} the state
 */
function readState() {
  try {
    if (stateFile) {
      return JSON.parse(readFileSync(stateFile, "utf8"));
    }
    if (inlineState) {
      return JSON.parse(inlineState);
    }
  } catch (error) {
    process.stderr.write(`conduit: the state could not be read (${error.message})\n`);
  }
  return {};
}

/**
 * Publish the state of the surface to the near end.
 *
 * The near end seals it; this server never sees a key.
 *
 * @returns {Promise<void>}
 */
async function publishState() {
  if (!client) {
    return;
  }
  await client.publish(STATE_SCHEMA, latest.payload);
}

/**
 * How the tunnel is.
 *
 * The near end says so with the welcome frame: that is the broker
 * saying the tunnel is up.  There is nothing to ask and nothing to
 * wait for — a plugin publishes when it has something to say, and the
 * near end holds it until there is a key to seal it under.
 *
 * @returns {object|null} the state of the tunnel, when it is known
 */
function askStatus() {
  return lastStatus;
}

/**
 * The tool that reads the surface.
 *
 * The state it answers with is the state the near end last saw: either
 * what the surface published, or what the other end of the tunnel
 * published and the near end opened.
 *
 * @returns {Promise<object>} the result of the tool
 */
async function readSurface() {
  const status = await askStatus();
  if (!status) {
    return toolResult(
      "the near end did not answer, so the state of the surface is unknown",
      true,
    );
  }
  return toolResult(eventToText(latest));
}

/**
 * The revision to speak.
 *
 * The handshake era names the revision in the `initialize`; the modern
 * era names it in the metadata of every request.
 *
 * @param {object} params the parameters of the request
 * @returns {string} the revision
 */
function revision(params) {
  const meta = (params && params._meta) || {};
  const named =
    meta["io.modelcontextprotocol/protocolVersion"] || params?.protocolVersion;
  if (typeof named === "string" && SUPPORTED.includes(named)) {
    return named;
  }
  return HANDSHAKE[HANDSHAKE.length - 1];
}

/**
 * Answer a request of the client.
 *
 * @param {string} method the method of the request
 * @param {object} params the parameters of the request
 * @returns {Promise<object>} the result
 */
async function call(method, params) {
  switch (method) {
    case "initialize":
      return {
        protocolVersion: revision(params),
        capabilities: { tools: { listChanged: false }, resources: {} },
        serverInfo: SERVER_INFO,
      };
    case "server/discover":
      return {
        protocolVersions: SUPPORTED,
        capabilities: { tools: { listChanged: false }, resources: {} },
        serverInfo: SERVER_INFO,
      };
    case "ping":
      return {};
    case "tools/list":
      return { tools: [READ_TOOL] };
    case "tools/call": {
      if (!params || params.name !== TOOL_NAME) {
        const error = new Error(`there is no tool called ${params?.name}`);
        error.code = -32602;
        throw error;
      }
      return readSurface();
    }
    case "resources/list":
      return { resources: [STATE_RESOURCE] };
    case "resources/read": {
      if (!params || params.uri !== SURFACE_URI) {
        const error = new Error(`there is no resource called ${params?.uri}`);
        error.code = -32602;
        throw error;
      }
      return {
        contents: [
          {
            uri: SURFACE_URI,
            mimeType: STATE_RESOURCE.mimeType,
            text: JSON.stringify(latest.payload, null, 2),
          },
        ],
      };
    }
    default: {
      const error = new Error(`there is no method called ${method}`);
      error.code = -32601;
      throw error;
    }
  }
}

/** Write a message to the client, on stdout, as one line. */
function send(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

/** Answer a request, or say that it failed. */
async function handle(message) {
  const { id, method, params } = message;
  if (id === undefined || id === null) {
    // A notification: there is nothing to answer.
    return;
  }
  try {
    const result = await call(method, params || {});
    send({ jsonrpc: "2.0", id, result });
  } catch (error) {
    send({
      jsonrpc: "2.0",
      id,
      error: {
        code: error.code || -32603,
        message: String(error.message || error),
      },
    });
  }
}

/** Read the requests of the client, one line at a time. */
let buffer = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => {
  buffer += chunk;
  let index = buffer.indexOf("\n");
  while (index >= 0) {
    const line = buffer.slice(0, index).trim();
    buffer = buffer.slice(index + 1);
    if (line) {
      let message;
      try {
        message = JSON.parse(line);
      } catch (error) {
        process.stderr.write(`conduit: that was not JSON (${error.message})\n`);
        message = null;
      }
      if (message) {
        handle(message);
      }
    }
    index = buffer.indexOf("\n");
  }
});
process.stdin.on("end", () => {
  process.exit(0);
});

/** Start: connect to the near end, and publish what the app knows. */
async function start() {
  if (!client) {
    process.stderr.write("conduit: there is no near end socket\n");
    return;
  }
  client.on((frame) => {
    if (frame.t === "publish" || frame.t === "result") {
      // The other end of the tunnel published something, and the near
      // end opened it: the state of the surface is that now.
      latest = {
        schema: frame.s || STATE_SCHEMA,
        channel: frame.c || channel,
        payload: frame.p,
      };
      return;
    }
    if (frame.t === "welcome") {
      // The broker welcomed the near end: the tunnel is up.
      lastStatus = frame;
      return;
    }
    if (frame.t === "error") {
      process.stderr.write(
        `conduit: the near end said: ${(frame.p && frame.p.message) || "an error"}\n`,
      );
    }
  });

  // The state is published at once.  The near end holds it until the
  // two ends have agreed their keys, and sends it then.
  await publishState();

  // And again whenever the app's state changes.
  let published = JSON.stringify(latest.payload);
  setInterval(() => {
    const state = readState();
    const text = JSON.stringify(state);
    if (text === published) {
      return;
    }
    published = text;
    latest = { schema: STATE_SCHEMA, channel, payload: state };
    publishState().catch((error) => {
      process.stderr.write(`conduit: the publish failed (${error.message})\n`);
    });
  }, 1000).unref();
}

start().catch((error) => {
  process.stderr.write(`conduit: the server could not start (${error.message})\n`);
  process.exit(1);
});
