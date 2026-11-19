// What the Stream Deck surface publishes.
//
// The plugin has two halves: this one, which knows what a pressed key
// says, and `index.js`, which is the part the Stream Deck loads.  The
// split is deliberate: this half has no dependency on the Stream Deck
// and no dependency on its SDK, so the check can drive the very same
// code the plugin drives.
//
// What is published is plaintext.  The plugin never seals anything and
// never holds a key: the near end does the sealing.

/** The schema of a key. */
const KEY_SCHEMA = "dev.conduit.streamdeck.key/v1";

/**
 * What is said about a pressed key.
 *
 * A key says: which key it is (the row and the column of the grid), on
 * which device, in which state, and that it went down.
 *
 * @param {object} key the key
 * @returns {object} the payload
 */
function keyPayload(key) {
  return {
    action: key.action || "",
    context: key.context || "",
    device: key.device || "",
    controller: key.controller || "",
    row: Number.isInteger(key.row) ? key.row : -1,
    column: Number.isInteger(key.column) ? key.column : -1,
    state: Number.isInteger(key.state) ? key.state : 0,
    isDown: Boolean(key.isDown),
  };
}

/**
 * Publish a pressed key.
 *
 * @param {object} client the client of the near end
 * @param {object} key the key
 * @returns {Promise<void>}
 */
async function publishKey(client, key) {
  await client.publish(KEY_SCHEMA, keyPayload(key));
}

module.exports = { KEY_SCHEMA, keyPayload, publishKey };
