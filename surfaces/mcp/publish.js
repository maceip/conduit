// What the MCP surface publishes, and what a tool result looks like.
//
// The MCP server (`server.mjs`) is the plugin of this surface: it holds
// the socket of the near end, publishes what the app knows to it, and
// answers the tool `read_surface` with the state of the surface.
//
// This module is the half that knows what the surface says and what an
// MCP tool result is.  It has no dependency on the server and none on
// the near end, so the check can drive the very same code the server
// drives.

/** The schema of the state of the surface. */
export const STATE_SCHEMA = "dev.conduit.mcp.state/v1";

/** The name of the tool that reads the surface. */
export const TOOL_NAME = "read_surface";

/** The resource the state is also a resource under. */
export const SURFACE_URI = "conduit://mcp/state@v1";

/** The tool, as MCP describes a tool. */
export const READ_TOOL = {
  name: TOOL_NAME,
  title: "Read the surface",
  description:
    "Read what the Conduit surface knows: the state of the surface, as the near end last saw it.",
  inputSchema: {
    type: "object",
    properties: {},
    additionalProperties: false,
  },
};

/** The resource, as MCP describes a resource. */
export const STATE_RESOURCE = {
  uri: SURFACE_URI,
  name: "surface-state",
  title: "The state of the surface",
  description: "The state of the Conduit surface, as JSON.",
  mimeType: "application/json",
};

/**
 * The result of a tool, as MCP says it: content, and text in it.
 *
 * @param {string} text the text of the result
 * @param {boolean} isError whether the tool failed
 * @returns {object} the result
 */
export function toolResult(text, isError = false) {
  const result = { content: [{ type: "text", text }] };
  if (isError) {
    result.isError = true;
  }
  return result;
}

/**
 * The text of a near end event, as the content of the tool.
 *
 * An event is what the near end says about the surface: either an
 * event it opened from the other end of the tunnel, or the state the
 * surface published itself.  The text is JSON, so that a client can
 * read it without knowing anything about Conduit.
 *
 * @param {object} event the event of the near end
 * @returns {string} the text
 */
export function eventToText(event) {
  const source = event || {};
  const payload = source.payload !== undefined ? source.payload : source;
  return JSON.stringify(
    {
      surface: source.schema || STATE_SCHEMA,
      channel: source.channel || "",
      state: payload === undefined ? null : payload,
    },
    null,
    2,
  );
}
