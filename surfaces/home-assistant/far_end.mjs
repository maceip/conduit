// The far end of the Home Assistant surface's check.
//
// The far end is a Conduit client: it subscribes to the surface and
// prints every event it receives, as JSON, one per line.  It is the
// Node client of the repository (`clients/node/client.mjs`), so the
// crypto on this side is the browser's WebCrypto and the crypto on the
// other side is the Rust near end: a frame that arrives opened was
// opened by two independent implementations of the same scheme.
//
// The settings come from the environment:
//   CONDUIT_BROKER   the tunnel address of the broker
//   CONDUIT_SURFACE  the identity of the surface
//   CONDUIT_CHANNEL  the channel of the surface
//   CONDUIT_TOKEN    the token the client presents
//   CONDUIT_SCOPES   the scopes the client asks for, comma separated
//   CONDUIT_EXPECT   how many events to wait for (default 1)

import { connect } from "../../clients/node/client.mjs";

const settings = {
  broker: process.env.CONDUIT_BROKER || "",
  surface: process.env.CONDUIT_SURFACE || "",
  channel: process.env.CONDUIT_CHANNEL || process.env.CONDUIT_SURFACE || "",
  token: process.env.CONDUIT_TOKEN || "",
  scopes: (process.env.CONDUIT_SCOPES || "").split(",").filter(Boolean),
};
const expected = Number(process.env.CONDUIT_EXPECT || "1");

const client = await connect(settings);

// The two ends agree their keys when they have said hello to each
// other, and the publish that follows is sealed under that key.  The
// handshake is given a moment to finish before the check publishes
// anything.
let agreed = false;
client.onFrame((frame) => {
  if (frame.kind === "agreed") {
    agreed = true;
  }
});
const deadline = Date.now() + 5000;
while (!agreed && Date.now() < deadline) {
  await new Promise((resolve) => setTimeout(resolve, 25));
}
console.log(JSON.stringify({ ready: true, agreed }));

for (let seen = 0; seen < expected; seen += 1) {
  const event = await client.next("publish", 15000);
  console.log(JSON.stringify({ event }));
}

client.close();
process.exit(0);
