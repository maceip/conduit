---
name: conduit-surface
description: Read what the Conduit surface knows about the current workspace — the workspace root, the files that are open and the session — through the bundled `conduit-surface_state` MCP server. Use it when the user asks what the workspace looks like right now, or when a task depends on the state of the workspace rather than on the files on disk.
---

# The Conduit surface

This plugin publishes what Antigravity's workspace knows to a Conduit
client, and gives you the same state as an MCP tool.

## What the surface is

A **surface** is a place a Conduit client can talk to.  Each surface has
three parts, and they are the same whatever the surface is:

| the part | what it is |
|---|---|
| **the app** | Antigravity.  The thing the user has open. |
| **the plugin** | the code inside the app that reads what the app knows. |
| **the near end** | the process that holds the socket to the broker and does the sealing. |

The plugin never sees a key and never holds a socket to the broker.  It
publishes what the app knows, in plain JSON, to the near end; the near
end seals it and sends it to the broker; the broker forwards it without
opening it; the client at the far end opens it with the key only the two
ends share.

The MCP server this plugin ships (`mcp/server.mjs`) is that plugin: it
holds the socket of the near end, publishes the state of the workspace to
it, and answers you.

## The tool

One tool is contributed, by the `conduit-surface_state` server:

- `read_surface` — the state of the surface, as JSON text.

Call it when the user asks what the workspace looks like right now.  The
answer is JSON with three parts:

```json
{
  "surface": "dev.conduit.antigravity.state/v1",
  "channel": "antigravity",
  "state": {
    "workspace": "/home/user/project",
    "open_files": ["src/main.rs", "README.md"],
    "session": "the name of the session"
  }
}
```

The same state is also a resource, `conduit://antigravity/workspace@v1`.

## What the surface publishes, and to whom

The state of the workspace, under the schema
`dev.conduit.antigravity.state/v1`, to the Conduit client the user has
paired with this surface.  The payload crosses the broker sealed: the
near end seals it, the broker forwards it without opening it, and only
the client at the far end can open it.

Nothing is published to the public internet, and nothing is published
that the user has not paired.  The surface carries the scope
`antigravity.workspace.read`: it may be read, and nothing else.

## If the tool fails

The tool answers with an error when the near end does not answer, which
means the near end is not running or the socket path is wrong.  The
socket path is the `CONDUIT_NEAR_END_SOCKET` variable of the plugin's
MCP server; the near end is the `conduit-near-end` program of the
Conduit repository, started with a configuration that names this
surface.
