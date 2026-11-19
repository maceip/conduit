// What the Thunderbird surface publishes.
//
// A WebExtension runs in the browser, so it cannot open a Unix socket:
// it publishes to the near end over the loopback instead.
//
// The payload lives here, and only here: the background script reads
// the message that is in front of the user and says what it is.

/** The schema of a message. */
export const MESSAGE_SCHEMA = "dev.conduit.thunderbird.message/v1";

/**
 * What is said about a message.
 *
 * @param {object} message the message: its headers and its folder
 * @returns {object} the payload
 */
export function messagePayload(message) {
  return {
    subject: message.subject || "",
    from: message.from || "",
    date: message.date || "",
    folder: message.folder || "",
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
export async function publishMessage(client, message) {
  await client.publish(MESSAGE_SCHEMA, messagePayload(message));
}

/**
 * The message that is in front of the user, as Thunderbird would say
 * it.
 *
 * @param {object} browser the WebExtension API object
 * @returns {Promise<object|null>} the message, or null when there is none
 */
export async function messageFromBrowser(browser) {
  if (!browser || !browser.messenger || !browser.messenger.messageList) {
    return null;
  }
  const listed = await browser.messenger.messageList.getDisplayedMessages();
  const first = listed && listed.messages ? listed.messages[0] : null;
  if (!first) {
    return null;
  }
  return {
    id: first.id || "",
    subject: first.subject || "",
    from: first.from || "",
    date: first.date ? new Date(first.date).toISOString() : "",
    folder: first.folder ? first.folder.name || "" : "",
  };
}
