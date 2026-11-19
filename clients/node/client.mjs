// The client of a tunnel, in Node.
//
// The browser's own client is the extension's background page.  This is
// the same thing for Node: it is what a surface's check talks to, and
// what a script would use to read a surface.
//
// It is also the other half of the cross-language check: the crypto
// here is the browser's WebCrypto, and the far end is the Rust near
// end, so a frame that arrives opened was opened by two independent
// implementations of the same scheme.

import {
  agree,
  fromBase64Url,
  generateKeyPair,
  open,
  toBase64Url,
} from "../shared/sealing.js";

/**
 * Connect to a surface and subscribe to it.
 *
 * @param {object} options
 * @param {string} options.broker the tunnel address of the broker
 * @param {string} options.surface the identity of the surface
 * @param {string} options.channel the channel of the surface
 * @param {string} options.token the token the client presents
 * @param {string[]} options.scopes the scopes the client asks for
 * @param {(event: object) => void} [options.onEvent] what to do with a frame
 * @returns {Promise<object>} the client
 */
export async function connect(options) {
  const { broker, surface, channel, token, scopes = [] } = options;
  const onEvent = options.onEvent || (() => {});

  const keys = await generateKeyPair();
  const prefix = crypto.getRandomValues(new Uint8Array(4));
  let sequence = 0;
  let tunnel = null;
  let peerPrefix = null;

  const nextSequence = () => {
    sequence += 1;
    return sequence;
  };

  const url = new URL(broker);
  url.searchParams.set("role", "client");
  url.searchParams.set("surface", surface);
  url.searchParams.set("token", token);

  const socket = new WebSocket(url);
  let onReady = null;
  let onRefusal = null;
  const ready = new Promise((resolve, reject) => {
    onReady = resolve;
    onRefusal = (frame) => {
      reject(new Error(`the broker refused the client: ${frame.p || frame.t}`));
    };
    socket.addEventListener("open", () => {
      socket.send(
        JSON.stringify({
          t: "hello",
          s: "dev.conduit.hello/v1",
          c: channel,
          seq: nextSequence(),
          p: JSON.stringify({
            role: "client",
            surface,
            resume: 0,
            crypto: {
              scheme: "p256-aes256gcm/v1",
              public: toBase64Url(keys.publicBytes),
              prefix: toBase64Url(prefix),
            },
          }),
        }),
      );
    });
    socket.addEventListener("error", () => reject(new Error("the broker did not answer")));
  });

  const events = [];
  let onFrame = null;

  socket.addEventListener("message", async (message) => {
    const frame = JSON.parse(message.data);
    if (frame.t === "welcome") {
      if (onReady) {
        onReady();
      }
      return;
    }
    if (frame.t === "error") {
      // The broker refused this client.  That is not a reason to wait.
      if (onRefusal) {
        onRefusal(frame);
      }
      return;
    }
    if (frame.t === "hello") {
      // The other end said hello: agree the keys of the tunnel.
      const hello = JSON.parse(frame.p);
      const shared = await agree(keys.privateKey, fromBase64Url(hello.crypto.public));
      tunnel = shared;
      peerPrefix = fromBase64Url(hello.crypto.prefix);
      socket.send(
        JSON.stringify({
          t: "subscribe",
          s: "dev.conduit.subscribe/v1",
          c: channel,
          seq: nextSequence(),
          p: JSON.stringify({ channel, scopes }),
        }),
      );
      if (onFrame) {
        onFrame({ kind: "agreed" });
      }
      return;
    }
    if (frame.t === "publish" && tunnel) {
      // What the extension sent, opened with the key of this direction.
      const bytes = await open(
        tunnel.toExtension,
        peerPrefix,
        frame.seq,
        fromBase64Url(frame.p),
      );
      const payload = bytesToText(bytes);
      const event = {
        kind: "publish",
        schema: frame.s,
        channel: frame.c,
        seq: frame.seq,
        payload,
        sealed: frame.p,
      };
      events.push(event);
      onEvent(event);
      if (onFrame) {
        onFrame(event);
      }
      return;
    }
    if (onFrame) {
      onFrame({ kind: frame.t, frame });
    }
  });

  await ready;

  return {
    /** The frames that have arrived. */
    events,
    /** Say what happens when a frame arrives. */
    onFrame(handler) {
      onFrame = handler;
    },
    /** Publish something of our own, sealed under the tunnel's key. */
    async publish(schema, channel, payload) {
      const { seal } = await import("../shared/sealing.js");
      const seq = nextSequence();
      const sealed = await seal(tunnel.toClient, prefix, seq, JSON.stringify(payload));
      socket.send(
        JSON.stringify({
          t: "publish",
          s: schema,
          c: channel,
          seq,
          p: toBase64Url(sealed),
        }),
      );
    },
    /** Wait for the next frame of that kind. */
    async next(kind = "publish", timeoutMs = 10000) {
      const started = Date.now();
      for (;;) {
        const found = events.find((event) => event.kind === kind);
        if (found) {
          return found;
        }
        if (Date.now() - started > timeoutMs) {
          throw new Error(`no ${kind} arrived in ${timeoutMs}ms`);
        }
        await new Promise((resolve) => setTimeout(resolve, 25));
      }
    },
    /** Close the socket. */
    close() {
      socket.close();
    },
  };
}

/** The bytes as text, and as JSON when they are JSON. */
function bytesToText(bytes) {
  const text = new TextDecoder().decode(bytes);
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}
