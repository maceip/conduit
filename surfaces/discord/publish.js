// What the Discord surface publishes.
//
// The plugin has two halves: this one, which knows what a Discord
// message is and what to say about it, and `bot.js`, which is the part
// that talks to Discord.  The split is deliberate: this half has no
// dependency on Discord, so the check can drive the very same code the
// bot drives.
//
// What is published is plaintext.  The plugin never seals anything and
// never holds a key: the near end does the sealing.

/** The schema of a message. */
const MESSAGE_SCHEMA = "dev.conduit.discord.message/v1";

/**
 * What is said about a message.
 *
 * A Discord message says: the guild it was said in, the channel, who
 * said it, and what was said.
 *
 * @param {object} message the message
 * @returns {object} the payload
 */
function messagePayload(message) {
  return {
    guild: message.guild || "",
    channel: message.channel || "",
    author: message.author || "",
    content: message.content || "",
    id: message.id || "",
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
