// The check of the Figma surface.
//
// A Figma plugin cannot open a Unix socket, so this surface is carried
// over the loopback: the check starts the near end, connects to it
// with the near end's web client — the same one the iframe uses — and
// the client at the far end must receive the selection.

import { createServer } from "node:net";
import { check, declaration, withSurface } from "../../scripts/surfaces/harness.mjs";
import { publishSelection, selectionPayload, SELECTION_SCHEMA } from "./publish.js";

/** A port nothing is on. */
function aFreePort() {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.on("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });
}

await check("the Figma surface publishes the selection over the loopback", async () => {
  const surface = declaration(new URL(".", import.meta.url).pathname);
  surface.tcp = await aFreePort();

  await withSurface(surface, async ({ client }) => {
    const { connect } = await import("../../clients/shared/near-end-web.js");
    const plugin = connect(`ws://127.0.0.1:${surface.tcp}`);
    let complaint = null;
    plugin.on((message) => {
      if (message.t === "error") {
        complaint = message.message;
      }
    });

    await publishSelection(plugin, [
      { id: "1:2", name: "Frame", type: "FRAME" },
      { id: "3:4", name: "Text", type: "TEXT" },
    ]);

    if (complaint) {
      throw new Error(`the near end refused the publish: ${complaint}`);
    }
    const event = await client.next("publish");
    if (event.schema !== SELECTION_SCHEMA) {
      throw new Error(`the schema was ${event.schema}`);
    }
    const payload = typeof event.payload === "string" ? JSON.parse(event.payload) : event.payload;
    if (payload.count !== 2 || payload.nodes[0].name !== "Frame") {
      throw new Error(`the payload was ${JSON.stringify(payload)}`);
    }
    if (event.sealed === event.payload) {
      throw new Error("the payload crossed the broker in the clear");
    }

    // The half that reads Figma: the same function the main thread
    // calls, driven with the shape Figma would give it.
    const selection = selectionPayload([{ id: "9:9", name: "Ellipse", type: "ELLIPSE" }]);
    if (selection.count !== 1 || selection.nodes[0].type !== "ELLIPSE") {
      throw new Error("the payload has the wrong shape");
    }
    plugin.close();
  });
});
