// The check of the Safari surface.
//
// Safari's background page is unloaded after about thirty seconds of
// silence, and a loopback socket does not survive that.  So on Safari
// the socket lives in a native process, and the extension talks to it
// over the browser's native messaging protocol.
//
// The check starts that process and speaks to it the way Safari does:
// a four-byte big-endian length, then that many bytes of JSON, on the
// standard input and the standard output of the process.

import { check, declaration, withNativeSurface } from "../../scripts/surfaces/harness.mjs";
import { TAB_SCHEMA } from "./publish.js";

await check("the Safari surface publishes the tab in front of the user", async () => {
  const surface = declaration(new URL(".", import.meta.url).pathname);
  await withNativeSurface(surface, async ({ native, client }) => {
    // The browser says where the broker is, and the near end answers
    // with the state of the tunnel.
    const status = await native.next("status");
    if (!status.agreed) {
      throw new Error("the near end did not agree the keys of the tunnel");
    }

    native.send({
      op: "publish",
      schema: TAB_SCHEMA,
      payload: {
        title: "Conduit",
        url: "https://example.test/conduit",
        active: true,
      },
    });

    const event = await client.next("publish");
    if (event.schema !== TAB_SCHEMA) {
      throw new Error(`the schema was ${event.schema}`);
    }
    const payload = typeof event.payload === "string" ? JSON.parse(event.payload) : event.payload;
    if (payload.title !== "Conduit" || payload.url !== "https://example.test/conduit") {
      throw new Error(`the payload was ${JSON.stringify(payload)}`);
    }
    if (event.sealed === event.payload) {
      throw new Error("the payload crossed the broker in the clear");
    }
  });
});
