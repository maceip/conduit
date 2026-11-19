// The Discord bot.
//
// A Discord bot is a remote app: it opens the Gateway for the real-time
// events, and uses the HTTP API for the rest (registering a slash
// command).  Discord has no manifest file: `bot.json` is the bot's own
// configuration, not something Discord reads.
//
// The bot holds no key and opens no socket to the broker.  It publishes
// what Discord says, in plain JSON, to the near end, which does the
// sealing.  The publishing half is in `publish.js`, which knows nothing
// about Discord: this file is the half that talks to Discord.
//
// Discord's own SDK is not used here: the Gateway is a WebSocket and a
// handful of JSON messages, and the built-in `WebSocket` and `fetch` of
// Node are enough.

const { connect } = require("../../clients/native/near-end.js");
const { publishMessage, MESSAGE_SCHEMA } = require("./publish.js");
const config = require("./bot.json");

/** Where the near end is, if the user has said. */
function socketPath() {
  return process.env.CONDUIT_SOCKET || "";
}

/**
 * Ask Discord for the Gateway to connect to.
 *
 * @returns {Promise<string>} the URL of the Gateway WebSocket
 */
async function gatewayUrl() {
  const token = process.env.DISCORD_BOT_TOKEN || "";
  if (!token) {
    // The documented endpoint, used as the placeholder.  It will not
    // let the bot in without a token, and that is said out loud.
    console.warn(
      `DISCORD_BOT_TOKEN is not set: using the documented endpoint ${config.gateway_endpoint}, which needs a token.`,
    );
    return config.gateway_endpoint;
  }
  const response = await fetch(`${config.api_base}/gateway/bot`, {
    headers: { Authorization: `Bot ${token}` },
  });
  const body = await response.json();
  if (!body.url) {
    throw new Error(`Discord did not give a gateway: ${body.message || response.status}`);
  }
  return body.url;
}

/**
 * What a Discord message says.
 *
 * @param {object} message the message of the MESSAGE_CREATE event
 * @returns {object} the message
 */
function messageOf(message) {
  return {
    guild: message.guild_id || "",
    channel: message.channel_id || "",
    author: (message.author || {}).id || "",
    content: message.content || "",
    id: message.id || "",
  };
}

/**
 * Register the slash command of the bot with Discord.
 *
 * The path is the documented one: the commands of an application live
 * under the application.
 *
 * @param {string} token the bot token
 * @returns {Promise<object>} what Discord answered
 */
async function registerSlashCommand(token) {
  const url = `${config.api_base}/applications/${config.application_id}/commands`;
  const response = await fetch(url, {
    method: "POST",
    headers: {
      Authorization: `Bot ${token}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify([
      {
        name: config.slash_command.name,
        description: config.slash_command.description,
        type: config.slash_command.type,
      },
    ]),
  });
  const body = await response.json();
  if (!response.ok) {
    throw new Error(`Discord did not take the command: ${JSON.stringify(body)}`);
  }
  return body;
}

async function main() {
  const socket = socketPath();
  if (!socket) {
    // Nothing to publish to until the user says where the near end is.
    console.error("CONDUIT_SOCKET is not set: there is no near end to publish to.");
    return;
  }
  const client = connect(socket);
  const token = process.env.DISCORD_BOT_TOKEN || "";

  if (token) {
    // The slash command is registered through the HTTP API, as the
    // application commands of Discord are.
    await registerSlashCommand(token);
  } else {
    console.warn("DISCORD_BOT_TOKEN is not set: the slash command is not registered.");
  }

  const url = await gatewayUrl();
  console.log(`the Discord bot is connecting to ${url}`);

  const ws = new WebSocket(url);
  let sequence = null;
  ws.addEventListener("error", () => {
    console.error("the Gateway connection failed");
  });
  ws.addEventListener("message", (event) => {
    let frame = null;
    try {
      frame = JSON.parse(event.data);
    } catch {
      return;
    }
    handle(frame).catch((error) => {
      console.error(`the Discord event was not published: ${error.message}`);
    });
  });

  ws.addEventListener("open", () => {
    // The handshake: the bot says who it is and what it wants to see.
    ws.send(
      JSON.stringify({
        op: 2,
        d: {
          token,
          intents: config.intents,
          properties: { os: process.platform, browser: "conduit", device: "conduit" },
        },
      }),
    );
  });

  /**
   * What to do with one frame from Discord.
   *
   * @param {object} frame the frame
   * @returns {Promise<void>}
   */
  async function handle(frame) {
    if (typeof frame.s === "number") {
      sequence = frame.s;
    }
    if (frame.op === 1) {
      // Discord asks for a heartbeat: it is answered with the last
      // sequence that arrived.
      ws.send(JSON.stringify({ op: 1, d: sequence }));
      return;
    }
    if (frame.op === 11) {
      return; // The heartbeat was answered.
    }
    if (frame.op !== 0) {
      return;
    }
    if (frame.t === "MESSAGE_CREATE") {
      await publish(messageOf(frame.d || {}));
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

module.exports = { MESSAGE_SCHEMA, messageOf, registerSlashCommand };

// The bot runs when it is started, not when it is required.
if (require.main === module) {
  main().catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
}
