// The client of the near end, for a plugin that runs inside a web page.
//
// A web extension, a Figma plugin and a panel in an editor that is
// built on a web page cannot open a Unix socket.  So the near end also
// listens on the loopback and speaks the same protocol there: one JSON
// message per web socket message.
//
//   plugin  ->  {"op":"publish","schema":"dev.conduit.browser.tab/v1","payload":{…}}
//   near end ->  {"op":"event","schema":"…","channel":"…","seq":7,"payload":{…}}

/**
 * The client of a near end over a web socket.
 *
 * @param {string} url the address of the near end, for example
 *   `ws://127.0.0.1:8788`
 * @returns {NearEndWebClient}
 */
export function connect(url) {
  return new NearEndWebClient(url);
}

/** The client of a near end over a web socket. */
export class NearEndWebClient {
  #socket;
  #handlers = new Set();
  #queue = [];
  #open = false;
  #agreed = false;

  constructor(url) {
    this.#socket = new WebSocket(url);
    this.#socket.addEventListener("open", () => {
      this.#open = true;
      for (const message of this.#queue) {
        this.#socket.send(message);
      }
      this.#queue = [];
    });
    this.#socket.addEventListener("message", (event) => {
      let message;
      try {
        message = JSON.parse(event.data);
      } catch {
        return;
      }
      if (message.op === "status" && message.agreed) {
        this.#agreed = true;
      }
      for (const handler of this.#handlers) {
        handler(message);
      }
    });
    this.#socket.addEventListener("close", () => {
      this.#open = false;
    });
    this.#socket.addEventListener("error", () => {});
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
    this.#socket.close();
  }

  /** Whether the socket to the near end is up. */
  get connected() {
    return this.#open;
  }

  #send(message) {
    const body = JSON.stringify(message);
    if (!this.#open) {
      this.#queue.push(body);
      return;
    }
    this.#socket.send(body);
  }
}
