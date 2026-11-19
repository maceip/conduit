// The check of the Thunderbird surface.
//
// A WebExtension cannot open a Unix socket, so this surface is carried
// over the loopback: the check starts the near end, connects to it
// with the near end's web client — the same one the background script
// uses — and the client at the far end must receive the message.

import { createServer } from "node:net";
import { check, declaration, withSurface } from "../../scripts/surfaces/harness.mjs";
import { publishMessage, messagePayload, MESSAGE_SCHEMA } from "./publish.js";

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

await check("the Thunderbird surface publishes the message over the loopback", async () => {
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

    await publishMessage(plugin, {
      id: "42",
      subject: "the near end",
      from: "someone@example.test",
      date: "2026-11-19T12:00:00.000Z",
      folder: "Inbox",
    });

    if (complaint) {
      throw new Error(`the near end refused the publish: ${complaint}`);
    }
    const event = await client.next("publish");
    if (event.schema !== MESSAGE_SCHEMA) {
      throw new Error(`the schema was ${event.schema}`);
    }
    const payload = typeof event.payload === "string" ? JSON.parse(event.payload) : event.payload;
    if (payload.subject !== "the near end" || payload.folder !== "Inbox") {
      throw new Error(`the payload was ${JSON.stringify(payload)}`);
    }
    if (event.sealed === event.payload) {
      throw new Error("the payload crossed the broker in the clear");
    }

    // The half that reads Thunderbird: the same function the background
    // script calls, driven with the shape Thunderbird would give it.
    const message = messagePayload({ subject: "other" });
    if (message.subject !== "other" || message.from !== "") {
      throw new Error("the payload has the wrong shape");
    }
    plugin.close();
  });
});
