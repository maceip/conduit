// The check of the MCP surface.
//
// The MCP server is run as a child process, and the check speaks
// JSON-RPC to it over stdio, the way an MCP client does: `initialize`,
// then `tools/list`, then `tools/call`.  The tool's answer must carry
// the state of the surface, and the client at the far end of the tunnel
// must have received that state, sealed.

import { spawn } from "node:child_process";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { check, declaration, withSurface } from "../../scripts/surfaces/harness.mjs";
import { STATE_SCHEMA, TOOL_NAME, eventToText } from "./publish.js";

/** A client of an MCP server over stdio: JSON-RPC, one message per line. */
class Rpc {
  /**
   * @param {object} child the server, as a child process
   */
  constructor(child) {
    this.child = child;
    this.next = 1;
    this.waiting = new Map();
    this.buffer = "";
    this.errors = [];
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk) => this.receive(chunk));
    child.stderr.on("data", (chunk) => this.errors.push(chunk.toString()));
  }

  /** Take the messages out of a chunk of the server's stdout. */
  receive(chunk) {
    this.buffer += chunk;
    let index = this.buffer.indexOf("\n");
    while (index >= 0) {
      const line = this.buffer.slice(0, index).trim();
      this.buffer = this.buffer.slice(index + 1);
      if (line) {
        const message = JSON.parse(line);
        const waiting = this.waiting.get(message.id);
        if (waiting) {
          this.waiting.delete(message.id);
          waiting.resolve(message);
        }
      }
      index = this.buffer.indexOf("\n");
    }
  }

  /**
   * Make a request, and wait for the answer.
   *
   * @param {string} method the method of the request
   * @param {object} params the parameters of the request
   * @returns {Promise<object>} the answer
   */
  request(method, params) {
    const id = this.next;
    this.next += 1;
    const answer = new Promise((resolve, reject) => {
      this.waiting.set(id, { resolve, reject });
      setTimeout(() => {
        if (this.waiting.delete(id)) {
          reject(new Error(`the server did not answer ${method}`));
        }
      }, 15000);
    });
    this.child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
    return answer;
  }

  /**
   * Say something to the server, without an answer.
   *
   * @param {string} method the method of the notification
   * @param {object} params the parameters of the notification
   */
  notify(method, params) {
    this.child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method, params })}\n`);
  }
}

/** The state of the surface, as the tool answers it. */
function stateOf(answer) {
  if (!answer.result || !answer.result.content || !answer.result.content[0]) {
    throw new Error(`the answer was ${JSON.stringify(answer)}`);
  }
  if (answer.result.content[0].type !== "text") {
    throw new Error(`the content was ${JSON.stringify(answer.result.content[0])}`);
  }
  return JSON.parse(answer.result.content[0].text);
}

/** Wait a little. */
function sleep(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

/** Two values, the same, whatever order their keys are in. */
function same(left, right) {
  if (left === right) {
    return true;
  }
  if (typeof left !== typeof right || left === null || right === null) {
    return false;
  }
  if (typeof left !== "object") {
    return false;
  }
  if (Array.isArray(left) !== Array.isArray(right)) {
    return false;
  }
  if (Array.isArray(left)) {
    return left.length === right.length && left.every((item, index) => same(item, right[index]));
  }
  const keys = Object.keys(left);
  const otherKeys = Object.keys(right);
  if (keys.length !== otherKeys.length) {
    return false;
  }
  return keys.every((key) => same(left[key], right[key]));
}

await check(
  "the MCP surface exposes the state of the surface as an MCP tool",
  async () => {
    const here = new URL(".", import.meta.url).pathname;
    const surface = declaration(here);
    if (!(surface.schemas || []).includes(STATE_SCHEMA)) {
      throw new Error(`the surface does not declare ${STATE_SCHEMA}`);
    }
    await withSurface(surface, async ({ socket, client, directory }) => {
      // The state of the app, which the server publishes.
      const state = { surface: "example", items: ["alpha", "beta"], count: 2 };
      const stateFile = join(directory, "state.json");
      writeFileSync(stateFile, JSON.stringify(state));

      const server = spawn(process.execPath, [join(here, "server.mjs")], {
        env: {
          ...process.env,
          CONDUIT_SOCKET: socket,
          CONDUIT_CHANNEL: surface.channel,
          CONDUIT_STATE_FILE: stateFile,
        },
        stdio: ["pipe", "pipe", "pipe"],
      });

      try {
        const rpc = new Rpc(server);

        // The handshake, then the tools, then the call.
        const initialized = await rpc.request("initialize", {
          protocolVersion: "2025-06-18",
          capabilities: {},
          clientInfo: { name: "conduit-check", version: "0.1.0" },
        });
        if (!initialized.result || !initialized.result.serverInfo) {
          throw new Error(`the handshake was ${JSON.stringify(initialized)}`);
        }
        await rpc.notify("notifications/initialized");

        const tools = await rpc.request("tools/list");
        const names = (tools.result.tools || []).map((tool) => tool.name);
        if (!names.includes(TOOL_NAME)) {
          throw new Error(`the tools were ${JSON.stringify(names)}`);
        }

        const called = await rpc.request("tools/call", {
          name: TOOL_NAME,
          arguments: {},
        });
        const text = called.result.content[0].text;
        const expected = eventToText({
          schema: STATE_SCHEMA,
          channel: surface.channel,
          payload: state,
        });
        if (text !== expected) {
          throw new Error(`the tool said ${text}`);
        }
        const answered = stateOf(called);
        if (!same(answered.state, state)) {
          throw new Error(`the tool said ${JSON.stringify(answered)}`);
        }

        // The client at the far end received that state, sealed.
        const event = await client.next("publish");
        if (event.schema !== STATE_SCHEMA) {
          throw new Error(`the schema was ${event.schema}`);
        }
        const payload =
          typeof event.payload === "string" ? JSON.parse(event.payload) : event.payload;
        if (!same(payload, state)) {
          throw new Error(`the payload was ${JSON.stringify(payload)}`);
        }
        if (typeof event.sealed !== "string" || !event.sealed) {
          throw new Error("the payload was not sealed");
        }
        if (event.sealed === event.payload || event.sealed === JSON.stringify(payload)) {
          throw new Error("the payload crossed the broker in the clear");
        }

        // And the other way: what the other end of the tunnel publishes,
        // the near end opens and hands to the plugin, and the state of
        // the surface is that now.
        const update = { surface: "example", items: ["gamma"], count: 1 };
        await client.publish(STATE_SCHEMA, surface.channel, update);
        const deadline = Date.now() + 10000;
        let answered2 = null;
        while (Date.now() < deadline) {
          const again = await rpc.request("tools/call", {
            name: TOOL_NAME,
            arguments: {},
          });
          answered2 = stateOf(again);
          if (same(answered2.state, update)) {
            break;
          }
          await sleep(50);
        }
        if (!same(answered2.state, update)) {
          throw new Error(`the tool said ${JSON.stringify(answered2)}`);
        }
      } finally {
        server.kill("SIGKILL");
      }
    });
  },
);
