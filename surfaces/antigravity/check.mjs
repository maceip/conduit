// The check of the Antigravity surface.
//
// The MCP server that the plugin ships is run as a child process, and
// the check speaks JSON-RPC to it over stdio, the way Antigravity does:
// `initialize`, then `tools/list`, then `tools/call`.  The tool's answer
// must carry the state of the surface, and the client at the far end of
// the tunnel must have received that state, sealed.
//
// What is *not* checked here is Antigravity: the IDE was never started
// on this machine, so what is verified is the plugin's files and the
// MCP server it ships.

import { spawn } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { check, declaration, withSurface } from "../../scripts/surfaces/harness.mjs";

/** The schema of the state of the surface. */
const STATE_SCHEMA = "dev.conduit.antigravity.state/v1";

/** The name of the tool that reads the surface. */
const TOOL_NAME = "read_surface";

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

/**
 * Check the files of the plugin.
 *
 * `agy plugin validate` is the command that checks a plugin directory,
 * and the Antigravity CLI is not installed on this machine, so the few
 * rules of the documented format are checked here instead.
 *
 * @param {string} here the directory of the plugin
 */
function checkThePluginFiles(here) {
  const manifest = JSON.parse(readFileSync(join(here, "plugin.json"), "utf8"));
  for (const key of [
    "$schema",
    "name",
    "displayName",
    "version",
    "description",
    "logo",
    "suggestedPrompts",
    "skills",
    "author",
    "license",
    "keywords",
  ]) {
    if (manifest[key] === undefined) {
      throw new Error(`the manifest has no ${key}`);
    }
  }
  if (!/^[a-z0-9]+(-[a-z0-9]+)*$/.test(manifest.name)) {
    throw new Error(`the name of the plugin is ${manifest.name}`);
  }
  if (!existsSync(join(here, manifest.logo))) {
    throw new Error(`the logo ${manifest.logo} is not there`);
  }
  for (const skill of manifest.skills) {
    const file = join(here, skill, "SKILL.md");
    if (!existsSync(file)) {
      throw new Error(`the skill ${skill} is not there`);
    }
    const text = readFileSync(file, "utf8");
    const frontmatter = text.split("---")[1] || "";
    const name = /name:\s*(\S+)/.exec(frontmatter);
    const description = /description:\s*(.+)/.exec(frontmatter);
    if (!name || !description) {
      throw new Error(`the skill ${skill} has no frontmatter`);
    }
    if (name[1] !== skill.split("/").pop()) {
      throw new Error(`the skill is called ${name[1]} and lives in ${skill}`);
    }
    if (description[1].length > 1024) {
      throw new Error(`the description of the skill ${skill} is too long`);
    }
  }

  // The MCP servers of the plugin.
  const configuration = JSON.parse(readFileSync(join(here, "mcp_config.json"), "utf8"));
  if (!configuration.mcpServers) {
    throw new Error("the MCP configuration has no mcpServers");
  }
  for (const [key, server] of Object.entries(configuration.mcpServers)) {
    if (!server.command && !server.serverUrl) {
      throw new Error(`the MCP server ${key} has neither a command nor a serverUrl`);
    }
    for (const argument of server.args || []) {
      const path = argument.replace("${PLUGIN_ROOT}", here);
      if (path.startsWith("/") && !existsSync(path)) {
        throw new Error(`the MCP server ${key} runs ${path}, which is not there`);
      }
    }
  }
}

await check(
  "the Antigravity plugin's MCP server exposes the state of the surface",
  async () => {
    const here = new URL(".", import.meta.url).pathname;
    const surface = declaration(here);
    if (!(surface.schemas || []).includes(STATE_SCHEMA)) {
      throw new Error(`the surface does not declare ${STATE_SCHEMA}`);
    }
    checkThePluginFiles(here);
    await withSurface(surface, async ({ socket, client, directory }) => {
      // The state of the workspace, which the server publishes.
      const state = {
        workspace: "/home/user/project",
        open_files: ["src/main.rs", "README.md"],
        session: "conduit",
      };
      const stateFile = join(directory, "workspace.json");
      writeFileSync(stateFile, JSON.stringify(state));

      // The MCP server the plugin ships.
      const server = spawn(process.execPath, [join(here, "mcp", "server.mjs")], {
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
        if (!called.result || !called.result.content || !called.result.content[0]) {
          throw new Error(`the answer was ${JSON.stringify(called)}`);
        }
        if (called.result.content[0].type !== "text") {
          throw new Error(`the content was ${JSON.stringify(called.result.content[0])}`);
        }
        const answered = JSON.parse(called.result.content[0].text);
        if (answered.surface !== STATE_SCHEMA) {
          throw new Error(`the tool said ${JSON.stringify(answered)}`);
        }
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
      } finally {
        server.kill("SIGKILL");
      }
    });
  },
);
