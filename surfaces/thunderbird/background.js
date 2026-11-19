// The Thunderbird extension's background script.
//
// The extension runs in the browser, so it cannot open a Unix socket.
// It talks to the near end over the loopback, with the near end's own
// web client.
//
// The publishing half is in `publish.js`, which knows nothing about
// Thunderbird.

import { connect } from "../../clients/shared/near-end-web.js";
import { publishMessage, messageFromBrowser } from "./publish.js";

/** The port the near end listens on, unless the user said otherwise. */
const DEFAULT_PORT = 18791;

/**
 * Where the near end is.
 *
 * The extension asks its own storage, so the user can point it at a
 * near end that is not on the default port.
 *
 * @returns {Promise<string>} the address of the near end
 */
async function nearEndUrl() {
  const stored = await browser.storage.local.get("conduit.nearEndPort");
  const port = stored["conduit.nearEndPort"] || DEFAULT_PORT;
  return `ws://127.0.0.1:${port}`;
}

/** Publish the message that is in front of the user. */
async function publishTheMessage() {
  const client = connect(await nearEndUrl());
  client.on((frame) => {
    if (frame.t === "error") {
      console.warn("the near end:", frame.p && frame.p.message);
    }
  });
  const message = await messageFromBrowser(browser);
  if (message) {
    await publishMessage(client, message);
  }
  return client;
}

// The message list changes when the user opens another one.
browser.messages.onDisplayedMessagesChanged.addListener(() => {
  publishTheMessage().catch((error) => console.warn("conduit:", error));
});

publishTheMessage().catch((error) => console.warn("conduit:", error));
