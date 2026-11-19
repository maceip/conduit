// The check of the Stream Deck surface.
//
// The plugin's own publishing code runs against a live broker and a
// live near end, and the client at the far end must receive the key.
//
// What is not checked here is the Stream Deck: no device and no driver
// on this machine, so `index.js` has never run.

import { check, declaration, withSurface } from "../../scripts/surfaces/harness.mjs";
import { connect } from "../../clients/native/near-end.js";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { publishKey, KEY_SCHEMA } = require("./publish.js");

await check("the Stream Deck surface publishes the key that was pressed", async () => {
  const surface = declaration(new URL(".", import.meta.url).pathname);
  await withSurface(surface, async ({ socket, client }) => {
    const plugin = connect(socket);
    let complaint = null;
    plugin.on((message) => {
      if (message.t === "error") {
        complaint = message.message;
      }
    });


    await publishKey(plugin, {
      action: "dev.conduit.streamdeck.key",
      context: "a1b2c3d4",
      device: "streamdeck-1",
      controller: "Keypad",
      row: 1,
      column: 4,
      state: 0,
      isDown: true,
    });

    if (complaint) {
      throw new Error(`the near end refused the publish: ${complaint}`);
    }
    const event = await client.next("publish");
    if (event.schema !== KEY_SCHEMA) {
      throw new Error(`the schema was ${event.schema}`);
    }
    const payload = typeof event.payload === "string" ? JSON.parse(event.payload) : event.payload;
    if (payload.row !== 1 || payload.column !== 4 || payload.device !== "streamdeck-1") {
      throw new Error(`the payload was ${JSON.stringify(payload)}`);
    }
    if (payload.isDown !== true) {
      throw new Error(`the payload was ${JSON.stringify(payload)}`);
    }
    if (event.sealed === event.payload) {
      throw new Error("the payload crossed the broker in the clear");
    }
    if (String(event.sealed).includes("streamdeck-1")) {
      throw new Error("the key was in the frame on the wire");
    }
    plugin.close();
  });
});
