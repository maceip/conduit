# The MCP surface

An **MCP server** is a plugin surface: it contributes tools and resources
to a client that speaks the Model Context Protocol.  This one is the
plugin of a Conduit surface: it holds the socket of the near end, it
publishes what the app knows to the near end, and it answers the tool
`read_surface` with the state of the surface, as text content.

| the part | the file |
|---|---|
| the declaration | `surface.json` |
| the MCP server | `server.mjs` |
| what a tool result looks like | `publish.js` |
| the check | `check.mjs` |

## What it is, in MCP's words

MCP servers speak JSON-RPC 2.0 over stdio: one message per line, on
stdin and on stdout.  A client initializes with `initialize`, and then
calls tools with `tools/call`.  The result of a tool is
`{content: [{type: "text", text: "…"}]}`.

`server.mjs` answers `initialize`, `tools/list`, `tools/call`,
`resources/list`, `resources/read` and `ping`, and it speaks both eras of
the protocol: the handshake era (`2024-11-05` up to `2025-11-25`, where
the revision is negotiated in `initialize`) and the modern era
(`2026-07-28`, where the revision is declared in the metadata of every
request).  Nothing but the protocol is ever written to stdout; the near
end's chatter goes to stderr.

The state of the surface is also a resource, under the project's own
`conduit:` URI scheme: `conduit://mcp/state@v1`.

## What it publishes

The state of the app — a JSON document — under the schema
`dev.conduit.mcp.state/v1`.  The plugin publishes it to the near end,
which seals it; the broker forwards it without opening it; the client at
the far end opens it with the key only the two ends share.

The surface carries two scopes: `mcp.state.read`, which is the state
going out to the clients, and `mcp.state.write`, which is a client
publishing an update back to the surface.  The near end opens such an
update and hands it to the plugin as an event, and the state of the
surface is that from then on.

The state of the app is read from a JSON file, whose path is in
`CONDUIT_STATE_FILE` (or inline, in `CONDUIT_STATE`).  A real app would
read its own state; here it is a file, so that the check can put a state
in and see it come out.

## What was verified

**Verified on this machine.** `check.mjs` starts a real broker and a
real near end with this surface's declaration, runs `server.mjs` as a
child process, and speaks JSON-RPC to it over stdio: `initialize`,
`tools/list`, then `tools/call` for `read_surface`.  The tool's text
carries the state of the surface, and the client at the far end of the
tunnel received that same state, opened, with the payload sealed rather
than in the clear.  The other direction was exercised too: the client
published an update to the surface, the near end opened it and handed it
to the plugin as an event, and the tool then answered with the update.

`publish.js` — the shape of a tool result, and the function that turns
the near end's event into the MCP text content — is the very module the
server imports, and the check drives it as well.

**Not run.** No MCP client SDK was used: the check speaks JSON-RPC by
hand, so the server has not been run under a real MCP host.  The
extensions framework of SEP-2133 and the `ui://` scheme of MCP Apps are
not implemented here; the surface's identity follows the same
reverse-DNS convention (`dev.conduit.mcp.state/v1`), and nothing else of
that framework is used.
