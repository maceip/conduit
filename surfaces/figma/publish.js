// What the Figma surface publishes.
//
// A Figma plugin runs in a sandboxed iframe and is short-lived: the
// user invokes it and it goes away.  So it cannot hold a socket, and it
// publishes to the near end over the loopback instead.
//
// The payload lives here, and only here: the main thread sends the
// nodes it found, and the iframe — which holds the connection — says
// what they are.

/** The schema of a selection. */
export const SELECTION_SCHEMA = "dev.conduit.figma.selection/v1";

/**
 * What is said about a selection.
 *
 * @param {object[]} selection the nodes the user has selected
 * @returns {object} the payload
 */
export function selectionPayload(selection) {
  const nodes = Array.isArray(selection) ? selection : [];
  return {
    count: nodes.length,
    nodes: nodes.map((node) => ({
      id: node.id || "",
      name: node.name || "",
      type: node.type || "",
    })),
  };
}

/**
 * Publish a selection.
 *
 * @param {object} client the client of the near end
 * @param {object[]} selection the selection
 * @returns {Promise<void>}
 */
export async function publishSelection(client, selection) {
  await client.publish(SELECTION_SCHEMA, selectionPayload(selection));
}
