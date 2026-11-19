// The client of the near end: the half of a surface that lives in the app.
//
// The near end is a separate process that holds the socket to the
// broker and does the sealing.  The plugin talks to it over a Unix
// socket, in plain JSON, one message at a time — and the message is a
// frame of the tunnel's own shape:
//
//   plugin  ->  {"t":"publish","s":"dev.conduit.browser.tab/v1","c":"tabs","p":{…}}
//   near end ->  {"t":"publish","s":"…","c":"…","seq":7,"p":{…}}
//
// There is no second vocabulary.  The hop between a plugin and the
// near end carries the frames the tunnel carries, and the only thing
// that differs is that the payload of a published frame is plaintext:
// the near end seals it under the key of the tunnel before it leaves
// the machine, and the broker never sees it open.
//
// The plugin does not wait for the tunnel to be up.  The near end holds
// what it publishes until the two ends have agreed their keys, and
// sends it then.  The waiting happens once, in the near end, and not
// in every plugin in every language.
//
// This is the Node half.  A plugin that runs inside a web page cannot
// open a Unix socket, and uses `clients/shared/near-end-web.js`
// instead.

import net from "node:net";

/**
 * The client of a near end over a Unix socket.
 *
 * @param {string} socketPath the socket of the near end
 * @returns {NearEndClient}
 */
export function connect(socketPath) {
  return new NearEndClient(socketPath);
}

/** The client of a near end. */
export class NearEndClient {
  #socket;
  #buffer = Buffer.alloc(0);
  #handlers = new Set();
  #queue = [];
  #open = false;
  #channel = "";

  constructor(socketPath) {
    this.#socket = net.createConnection(socketPath);
    this.#socket.on("connect", () => {
      this.#open = true;
      // What was queued is already the body of a message: it is
      // written as it is, not made into a message again.
      for (const body of this.#queue) {
        this.#write(body);
      }
      this.#queue = [];
    });
    this.#socket.on("data", (chunk) => this.#receive(chunk));
    this.#socket.on("error", () => {});
    this.#socket.on("close", () => {
      this.#open = false;
    });
  }

  /**
   * The channel this plugin publishes on.
   *
   * The near end puts the channel of the surface on the frame anyway,
   * so this is only for the plugin's own sake.
   *
   * @param {string} channel the channel
   */
  setChannel(channel) {
    this.#channel = channel;
  }

  /**
   * Publish what the app knows.
   *
   * @param {string} schema the schema of the payload
   * @param {unknown} payload the payload, as JSON
   */
  publish(schema, payload) {
    this.#send({
      t: "publish",
      s: schema,
      c: this.#channel,
      seq: 0,
      p: payload,
    });
  }

  /**
   * Say where the broker is.
   *
   * The near end may be told this by a plugin that did the device
   * flow: the browser knows the secret, and the near end holds the
   * socket.
   *
   * @param {object} where the broker, the secret and the instance
   */
  connect(where) {
    this.#send({
      t: "connect",
      s: "",
      c: this.#channel,
      seq: 0,
      p: where,
    });
  }

  /**
   * Say what happens when the near end says something.
   *
   * @param {(frame: object) => void} handler
   * @returns {() => void} a function that takes the handler away
   */
  on(handler) {
    this.#handlers.add(handler);
    return () => this.#handlers.delete(handler);
  }

  /** Close the socket. */
  close() {
    this.#socket.end();
    this.#socket.destroy();
  }

  /** Whether the socket to the near end is up. */
  get connected() {
    return this.#open;
  }

  #send(message) {
    const body = Buffer.from(JSON.stringify(message), "utf8");
    if (!this.#open) {
      this.#queue.push(body);
      return;
    }
    this.#write(body);
  }

  /** Write the body of a message: a length, then the bytes. */
  #write(body) {
    const length = Buffer.alloc(4);
    length.writeUInt32BE(body.length, 0);
    this.#socket.write(Buffer.concat([length, body]));
  }

  #receive(chunk) {
    this.#buffer = Buffer.concat([this.#buffer, chunk]);
    for (;;) {
      if (this.#buffer.length < 4) {
        return;
      }
      const length = this.#buffer.readUInt32BE(0);
      if (this.#buffer.length < 4 + length) {
        return;
      }
      const body = this.#buffer.subarray(4, 4 + length).toString("utf8");
      this.#buffer = this.#buffer.subarray(4 + length);
      let frame;
      try {
        frame = JSON.parse(body);
      } catch {
        continue;
      }
      for (const handler of this.#handlers) {
        handler(frame);
      }
    }
  }
}
