// What the Slack surface publishes.
//
// The plugin has two halves: this one, which knows what a Slack message
// is and what to say about it, and `app.js`, which is the part that
// talks to Slack.  The split is deliberate: this half has no dependency
// on Slack and no dependency on Slack's SDK, so the check can drive the
// very same code the app drives.
//
// What is published is plaintext.  The plugin never seals anything and
// never holds a key: the near end does the sealing.

/** The schema of a message. */
const MESSAGE_SCHEMA = "dev.conduit.slack.message/v1";

/**
 * What is said about a message.
 *
 * A Slack message event says: the channel it was said in, who said it,
 * what was said, and — when it is a reply — the message it replies to.
 *
 * @param {object} message the message
 * @returns {object} the payload
 */
function messagePayload(message) {
  return {
    channel: message.channel || "",
    user: message.user || "",
    text: message.text || "",
    thread_ts: message.thread_ts || "",
    ts: message.ts || "",
  };
}

/**
 * Publish a message.
 *
 * @param {object} client the client of the near end
 * @param {object} message the message
 * @returns {Promise<void>}
 */
async function publishMessage(client, message) {
  await client.publish(MESSAGE_SCHEMA, messagePayload(message));
}

module.exports = { MESSAGE_SCHEMA, messagePayload, publishMessage };
