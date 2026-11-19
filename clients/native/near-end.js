// The client of the near end: the half of a surface that lives in the app.
//
// The near end is a separate process that holds the socket to the
// broker and does the sealing.  The plugin talks to it over a Unix
// socket, in plain JSON, one message at a time:
//
//   plugin  ->  {"op":"publish","schema":"dev.conduit.browser.tab/v1","payload":{…}}
//   near end ->  {"op":"event","schema":"…","channel":"…","seq":7,"payload":{…}}
//
// What the plugin publishes is plaintext on this socket.  What leaves
// the machine is not: the near end seals it under the key of the
// tunnel, and the broker never sees it open.
//
// This is the Node half.  A plugin that runs inside a web page cannot
// open a Unix socket, and uses `clients/shared/near-end.js` instead.

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
  #agreed = false;

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
   * Publish what the app knows.
   *
   * @param {string} schema the schema of the payload
   * @param {unknown} payload the payload, as JSON
   */
  publish(schema, payload) {
    this.#send({ op: "publish", schema, payload });
  }

  /** Ask how the tunnel is. */
  status() {
    this.#send({ op: "status" });
  }

  /**
   * Wait until the tunnel has a key.
   *
   * The two ends agree their keys when they have said hello to each
   * other, and that takes a moment.  A plugin that publishes before
   * then has nothing to seal the payload under, so it waits.
   *
   * @param {number} timeoutMs how long to wait
   * @returns {Promise<boolean>} whether the tunnel agreed
   */
  async ready(timeoutMs = 10000) {
    const started = Date.now();
    while (Date.now() - started < timeoutMs) {
      if (this.#agreed) {
        return true;
      }
      this.status();
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error("the near end did not agree the keys of the tunnel");
  }

  /**
   * Say what happens when the near end says something.
   *
   * @param {(message: object) => void} handler
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
      let message;
      try {
        message = JSON.parse(body);
      } catch {
        continue;
      }
      if (message.op === "status" && message.agreed) {
        this.#agreed = true;
      }
      for (const handler of this.#handlers) {
        handler(message);
      }
    }
  }
}
