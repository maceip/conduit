// The check of the Slack surface.
//
// The plugin's own publishing code runs against a live broker and a
// live near end, and the client at the far end must receive the
// message.
//
// What is not checked here is Slack: the app was never connected to
// slack.com from this machine, so `app.js` has never run.

import { check, declaration, withSurface } from "../../scripts/surfaces/harness.mjs";
import { connect } from "../../clients/native/near-end.js";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { publishMessage, MESSAGE_SCHEMA } = require("./publish.js");

await check("the Slack surface publishes the message of the channel", async () => {
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
      channel: "C0CONDUIT01",
      user: "U0CONDUIT01",
      text: "the near end seals this",
      thread_ts: "1717171717.000100",
      ts: "1717171716.000200",
    });

    if (complaint) {
      throw new Error(`the near end refused the publish: ${complaint}`);
    }
    const event = await client.next("publish");
    if (event.schema !== MESSAGE_SCHEMA) {
      throw new Error(`the schema was ${event.schema}`);
    }
    const payload = typeof event.payload === "string" ? JSON.parse(event.payload) : event.payload;
    if (payload.channel !== "C0CONDUIT01" || payload.text !== "the near end seals this") {
      throw new Error(`the payload was ${JSON.stringify(payload)}`);
    }
    if (payload.thread_ts !== "1717171717.000100") {
      throw new Error(`the thread was ${payload.thread_ts}`);
    }
    if (event.sealed === event.payload) {
      throw new Error("the payload crossed the broker in the clear");
    }
    if (String(event.sealed).includes(payload.text)) {
      throw new Error("the text of the message was in the frame on the wire");
    }
    plugin.close();
  });
});
