// The Slack app.
//
// A Slack app is remote: it runs on a server, and Slack sends it the
// events over a WebSocket (Socket Mode), so the app needs no public
// HTTP Request URL.
//
// The app holds no key and opens no socket to the broker.  It publishes
// what Slack says to the near end, which does the sealing.  The
// publishing half is in `publish.js`, which knows nothing about Slack:
// this file is the half that talks to Slack.
//
// Slack's own SDK is not used here: the Socket Mode protocol is a
// WebSocket and a handful of JSON messages, and the built-in `WebSocket`
// and `fetch` of Node are enough.

const { connect } = require("../../clients/native/near-end.js");
const { publishMessage, MESSAGE_SCHEMA } = require("./publish.js");

/**
 * The Socket Mode endpoint Slack documents.
 *
 * The real connection is opened by calling `apps.connections.open` with
 * an app-level token, which answers with a URL of this shape.  Without
 * a token there is nothing to connect to, so this is the placeholder:
 * the app says so and stops.
 */
const SOCKET_MODE_ENDPOINT = "wss://app.slack.com/link";

/** The Web API, for the one call Socket Mode needs. */
const API = "https://slack.com/api";

/** Where the near end is, if the user has said. */
function socketPath() {
  return process.env.CONDUIT_SOCKET || "";
}

/**
 * Ask Slack for the WebSocket to talk to.
 *
 * @returns {Promise<string>} the URL of the Socket Mode WebSocket
 */
async function socketModeUrl() {
  const token = process.env.SLACK_APP_TOKEN || "";
  if (!token) {
    // The documented endpoint, used as the placeholder.  It will not
    // answer without an app-level token, and that is said out loud.
    console.warn(
      `SLACK_APP_TOKEN is not set: using the documented endpoint ${SOCKET_MODE_ENDPOINT}, which needs a token.`,
    );
    return SOCKET_MODE_ENDPOINT;
  }
  const response = await fetch(`${API}/apps.connections.open`, {
    method: "POST",
    headers: { Authorization: `Bearer ${token}` },
  });
  const body = await response.json();
  if (!body.ok || !body.url) {
    throw new Error(`Slack did not open a connection: ${body.error || response.status}`);
  }
  return body.url;
}

/**
 * What a Slack message event says.
 *
 * @param {object} event the event
 * @returns {object} the message
 */
function messageOf(event) {
  return {
    channel: event.channel || "",
    user: event.user || "",
    text: event.text || "",
    thread_ts: event.thread_ts || "",
    ts: event.ts || "",
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
  const url = await socketModeUrl();
  console.log(`the Slack app is connecting to ${url}`);

  const ws = new WebSocket(url);
  ws.addEventListener("error", () => {
    // Slack closes and reopens the connection on its own; the reason is
    // in the log of the app.
    console.error("the Socket Mode connection failed");
  });
  ws.addEventListener("message", (event) => {
    let envelope = null;
    try {
      envelope = JSON.parse(event.data);
    } catch {
      return;
    }
    handle(envelope).catch((error) => {
      console.error(`the Slack event was not published: ${error.message}`);
    });
  });

  /**
   * What to do with one envelope from Slack.
   *
   * @param {object} envelope the envelope
   * @returns {Promise<void>}
   */
  async function handle(envelope) {
    if (envelope.type === "hello") {
      console.log(`the Socket Mode connection is up: ${envelope.num_connections} connection(s)`);
      return;
    }
    // Every envelope is acknowledged: Slack sends the ones that are not
    // acknowledged again.
    if (envelope.envelope_id) {
      ws.send(JSON.stringify({ envelope_id: envelope.envelope_id, payload: null }));
    }
    if (envelope.type === "events_api") {
      const event = (envelope.payload || {}).event || {};
      if (event.type === "message") {
        await publish(messageOf(event));
      }
      return;
    }
    if (envelope.type === "slash_commands") {
      // The slash command of the manifest: what the user said is said
      // in the channel too.
      const command = envelope.payload || {};
      await publish({
        channel: command.channel_id || "",
        user: command.user_id || "",
        text: `${command.command || ""} ${command.text || ""}`.trim(),
      });
    }
  }

  /**
   * Wait for the key of the tunnel, then publish.
   *
   * The near end cannot seal anything until the two ends have agreed
   * their keys, and a publish before that is refused.
   *
   * @param {object} message the message
   * @returns {Promise<void>}
   */
  async function publish(message) {
    await client.ready();
    await publishMessage(client, message);
  }
}

module.exports = { MESSAGE_SCHEMA, messageOf, socketModeUrl };

// The app runs when it is started, not when it is required.
if (require.main === module) {
  main().catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
}
