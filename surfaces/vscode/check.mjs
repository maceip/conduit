// The check of the VS Code surface.
//
// The plugin's own publishing code runs against a live broker and a
// live near end, and the client at the far end must receive the tab.

import { check, declaration, withSurface } from "../../scripts/surfaces/harness.mjs";
import { connect } from "../../clients/native/near-end.js";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { publishTab, TAB_SCHEMA } = require("./publish.js");

await check("the VS Code surface publishes the tab in front of the user", async () => {
  const surface = declaration(new URL(".", import.meta.url).pathname);
  await withSurface(surface, async ({ socket, client }) => {
    const plugin = connect(socket);
    let complaint = null;
    plugin.on((message) => {
      if (message.op === "error") {
        complaint = message.message;
      }
    });

    await plugin.ready();

    await publishTab(plugin, {
      title: "lib.rs",
      language: "rust",
      path: "/home/user/project/src/lib.rs",
      url: "file:///home/user/project/src/lib.rs",
      dirty: true,
    });

    if (complaint) {
      throw new Error(`the near end refused the publish: ${complaint}`);
    }
    const event = await client.next("publish");
    if (event.schema !== TAB_SCHEMA) {
      throw new Error(`the schema was ${event.schema}`);
    }
    const payload = typeof event.payload === "string" ? JSON.parse(event.payload) : event.payload;
    if (payload.title !== "lib.rs" || payload.language !== "rust") {
      throw new Error(`the payload was ${JSON.stringify(payload)}`);
    }
    if (event.sealed === event.payload) {
      throw new Error("the payload crossed the broker in the clear");
    }
    plugin.close();
  });
});
