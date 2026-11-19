// The check of the Obsidian surface.
//
// The plugin's own publishing code runs against a live broker and a
// live near end, and the client at the far end must receive the note.

import { createRequire } from "node:module";
import { check, declaration, withSurface } from "../../scripts/surfaces/harness.mjs";

const require = createRequire(import.meta.url);
const { publishNote, notePayload, NOTE_SCHEMA } = require("./publish.js");

await check("the Obsidian surface publishes the note in front of the user", async () => {
  const surface = declaration(new URL(".", import.meta.url).pathname);
  await withSurface(surface, async ({ socket, client }) => {
    const { connect } = await import("../../clients/native/near-end.js");
    const plugin = connect(socket);
    let complaint = null;
    plugin.on((message) => {
      if (message.op === "error") {
        complaint = message.message;
      }
    });
    await plugin.ready();

    await publishNote(plugin, {
      path: "notes/conduit.md",
      basename: "conduit",
      heading: "the near end",
      tags: ["conduit", "surface"],
      excerpt: "the near end holds the socket and does the sealing",
    });

    if (complaint) {
      throw new Error(`the near end refused the publish: ${complaint}`);
    }
    const event = await client.next("publish");
    if (event.schema !== NOTE_SCHEMA) {
      throw new Error(`the schema was ${event.schema}`);
    }
    const payload = typeof event.payload === "string" ? JSON.parse(event.payload) : event.payload;
    if (payload.basename !== "conduit" || payload.heading !== "the near end") {
      throw new Error(`the payload was ${JSON.stringify(payload)}`);
    }
    if (payload.tags.length !== 2) {
      throw new Error(`the tags were ${JSON.stringify(payload.tags)}`);
    }
    if (event.sealed === event.payload) {
      throw new Error("the payload crossed the broker in the clear");
    }

    // The half that reads Obsidian: the same function the plugin
    // calls, driven with the shape Obsidian would give it.
    const note = {
      path: "notes/other.md",
      basename: "other",
      tags: [],
      excerpt: "",
    };
    if (Object.keys(notePayload(note)).length !== 5) {
      throw new Error("the payload has the wrong shape");
    }
    plugin.close();
  });
});
