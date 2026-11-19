// The Stream Deck plugin.
//
// The Stream Deck driver loads a plugin and speaks to it over a
// WebSocket on the loopback.  The plugin holds no key and opens no
// socket to the broker: it publishes what the key says to the near
// end, which does the sealing.
//
// The publishing half is in `publish.js`, which knows nothing about the
// Stream Deck: this file is the half that talks to it.

const { connect } = require("../../clients/native/near-end.js");
const { publishKey, KEY_SCHEMA } = require("./publish.js");

/** The UUID of the action, as the manifest names it. */
const ACTION_UUID = "dev.conduit.streamdeck.key";

/** Where the near end is, if the user has said. */
function socketPath() {
  return process.env.CONDUIT_SOCKET || "";
}

/**
 * What a pressed key says.
 *
 * @param {object} event the keyDown event of the SDK
 * @returns {object} the key
 */
function keyOf(event) {
  const payload = event.payload || {};
  const coordinates = payload.coordinates || {};
  const action = event.action || {};
  const device = action.device || {};
  return {
    action: action.manifestId || "",
    context: action.id || "",
    device: device.id || "",
    controller: payload.controller || action.controllerType || "",
    row: coordinates.row,
    column: coordinates.column,
    state: payload.state,
    isDown: true,
  };
}

async function main() {
  const socket = socketPath();
  if (!socket) {
    // Nothing to publish to until the user says where the near end is.
    console.error("CONDUIT_SOCKET is not set: there is no near end to publish to.");
    return;
  }
  const client = connect(socket);

  // The SDK is published as an ES module, so it is loaded with an
  // import; the plugin itself is CommonJS.
  const { streamDeck: StreamDeck, SingletonAction } = await import("@elgato/streamdeck");

  // The action of the manifest: it is registered with the SDK, so that
  // the events of that action come to this handler.  The SDK registers
  // an action with `StreamDeck.actions.registerAction(action)`.
  class ConduitKeyAction extends SingletonAction {
    manifestId = ACTION_UUID;

    /**
     * Occurs when the user presses the key.
     *
     * @param {object} event the event
     * @returns {Promise<void>}
     */
    async onKeyDown(event) {
      await publish(keyOf(event));
    }
  }

  StreamDeck.actions.registerAction(new ConduitKeyAction());
  await StreamDeck.connect();

  /**
   * Wait for the key of the tunnel, then publish.
   *
   * The near end cannot seal anything until the two ends have agreed
   * their keys, and a publish before that is refused.
   *
   * @param {object} key the key
   * @returns {Promise<void>}
   */
  async function publish(key) {
    await client.ready();
    await publishKey(client, key);
  }
}

module.exports = { KEY_SCHEMA, keyOf };

// The plugin runs when it is started, not when it is required.
if (require.main === module) {
  main().catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
}
