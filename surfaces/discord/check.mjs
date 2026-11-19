// The check of the Discord surface.
//
// The plugin's own publishing code runs against a live broker and a
// live near end, and the client at the far end must receive the
// message.
//
// What is not checked here is Discord: the bot was never connected to
// discord.com from this machine, so `bot.js` has never run.

import { check, declaration, withSurface } from "../../scripts/surfaces/harness.mjs";
import { connect } from "../../clients/native/near-end.js";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { publishMessage, MESSAGE_SCHEMA } = require("./publish.js");

await check("the Discord surface publishes the message of the channel", async () => {
  const surface = declaration(new URL(".", import.meta.url).pathname);
  await withSurface(surface, async ({ socket, client }) => {
    const plugin = connect(socket);
    let complaint = null;
    plugin.on((message) => {
      if (message.t === "error") {
        complaint = message.message;
      }
    });


    await publishMessage(plugin, {
      guild: "1000000000000000001",
      channel: "2000000000000000002",
      author: "3000000000000000003",
      content: "the near end seals this",
      id: "4000000000000000004",
    });

    if (complaint) {
      throw new Error(`the near end refused the publish: ${complaint}`);
    }
    const event = await client.next("publish");
    if (event.schema !== MESSAGE_SCHEMA) {
      throw new Error(`the schema was ${event.schema}`);
    }
    const payload = typeof event.payload === "string" ? JSON.parse(event.payload) : event.payload;
    if (payload.guild !== "1000000000000000001" || payload.content !== "the near end seals this") {
      throw new Error(`the payload was ${JSON.stringify(payload)}`);
    }
    if (payload.channel !== "2000000000000000002" || payload.author !== "3000000000000000003") {
      throw new Error(`the payload was ${JSON.stringify(payload)}`);
    }
    if (event.sealed === event.payload) {
      throw new Error("the payload crossed the broker in the clear");
    }
    if (String(event.sealed).includes(payload.content)) {
      throw new Error("the text of the message was in the frame on the wire");
    }
    plugin.close();
  });
});
