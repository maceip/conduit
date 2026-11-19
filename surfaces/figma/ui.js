// The Figma plugin's iframe.
//
// The iframe is what can hold the connection to the near end: it runs
// in a browser, so it speaks to the near end over the loopback, with
// the near end's own web client.
//
// A Figma plugin has to be self-contained: the sandbox serves the
// plugin's own files and nothing else.  The build therefore inlines
// the near end's client into this file — see
// `scripts/build-extension.mjs`, which builds the `figma` target.

import { connect } from "../../clients/shared/near-end-web.js";
import { publishSelection } from "./publish.js";

/** The port the near end listens on, unless the user said otherwise. */
const DEFAULT_PORT = 18790;

/**
 * Where the near end is.
 *
 * @returns {string} the address of the near end
 */
function nearEndUrl() {
  const port = window.CONDUIT_NEAR_END_PORT || DEFAULT_PORT;
  return `ws://127.0.0.1:${port}`;
}

/** The iframe is up: hold the connection and publish what it is told. */
export function start() {
  const state = document.getElementById("state");
  const client = connect(nearEndUrl());

  client.on((message) => {
    if (message.op === "status" && message.agreed) {
      state.textContent = "the tunnel is up";
      return;
    }
    if (message.op === "error") {
      state.textContent = message.message;
    }
  });

  // The main thread tells the iframe what the user has selected.
  window.onmessage = async (event) => {
    const message = event.data.pluginMessage || event.data;
    if (!message || message.type !== "selection") {
      return;
    }
    await publishSelection(client, message.selection);
  };

  return client;
}

// In the editor there is a document; in the check there is not.
if (typeof document !== "undefined" && document.getElementById("state")) {
  start();
}
