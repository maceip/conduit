# The Antigravity surface

A **Google Antigravity plugin**.  A plugin is a self-contained directory
that packages skills, MCP servers, agents, rules and hooks into one
versioned unit; this one packages a skill and an MCP server, and the MCP
server is the plugin of a Conduit surface: it holds the socket of the
near end, it publishes what the workspace knows to the near end, and it
exposes the state of the surface to the agent as an MCP tool.

| the part | the file |
|---|---|
| the manifest Antigravity loads | `plugin.json` |
| the MCP server the plugin ships | `mcp/server.mjs` |
| the MCP servers of the plugin | `mcp_config.json` |
| the skill | `skills/conduit-surface/SKILL.md` |
| the logo | `assets/logo.svg` |
| the declaration | `surface.json` |
| the check | `check.mjs` |

## Installing it

Antigravity loads plugins from `.agents/plugins/<plugin-name>/` in the
project, or from `~/.gemini/config/plugins/<plugin-name>/` globally.
Copy this directory to one of those, under the name `conduit-surface`.

The plugin asks for one value: `CONDUIT_NEAR_END_SOCKET`, the path of
the Unix socket the Conduit near end listens on.  It is declared as a
variable of the MCP server in `mcp_config.json`, so Antigravity asks
for it when the plugin is enabled; the server it starts is
`${PLUGIN_ROOT}/mcp/server.mjs`, and — the server key is namespaced with
the name of the plugin — its tools are `conduit-surface_state`.

`agy plugin validate --strict ./surfaces/antigravity` is the command
that checks a plugin directory.  It was not run here: the Antigravity
CLI is not installed on this machine.

## What it publishes

The state of the workspace — the workspace root, the files that are
open, the session — under the schema `dev.conduit.antigravity.state/v1`.
The plugin publishes it to the near end, which seals it; the broker
forwards it without opening it; the client at the far end opens it with
the key only the two ends share.  The surface carries the scope
`antigravity.workspace.read`: it may be read, and nothing else.

The state of the workspace is read from a JSON file, whose path is in
`CONDUIT_STATE_FILE` (or inline, in `CONDUIT_STATE`).  A real
Antigravity would read its own workspace; here it is a file, so that the
check can put a state in and see it come out.

## What was verified

**Verified on this machine.** `check.mjs` starts a real broker and a
real near end with this surface's declaration, runs the plugin's own MCP
server (`mcp/server.mjs`) as a child process, and speaks JSON-RPC to it
over stdio: `initialize`, `tools/list`, then `tools/call` for
`read_surface`.  The tool's text carries the state of the surface, and
the client at the far end of the tunnel received that same state,
opened, with the payload sealed rather than in the clear.

**Not run.** Antigravity itself was never launched here, so what was
verified is the plugin's files and its MCP server, not the IDE loading
it.  The plugin was never installed by `agy plugin install`, never
validated by `agy plugin validate`, the skill was never loaded by an
agent, and the MCP server was never started by Antigravity — the check
started it by hand, over the same stdio transport Antigravity uses.

The plugin is built on the documented plugin system only.  Whether the
Antigravity IDE also loads stock VS Code `.vsix` extensions is **not**
documented by Google, and this plugin does not rely on it.
