// The client of the near end, for a plugin that runs inside a web page.
//
// A web extension, a Figma plugin and a panel in an editor that is
// built on a web page cannot open a Unix socket.  So the near end also
// listens on the loopback and speaks the same protocol there: one JSON
// frame per web socket message.
//
//   plugin  ->  {"t":"publish","s":"dev.conduit.browser.tab/v1","c":"tabs","p":{…}}
//   near end ->  {"t":"publish","s":"…","c":"…","seq":7,"p":{…}}
//
// There is no second vocabulary: the frames are the frames of the
// tunnel, and the payload of a published frame is plaintext on this
// socket because the near end is the one that seals it.
//
// The plugin does not wait for the tunnel to be up.  The near end holds
// what it publishes until the two ends have agreed their keys.

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
  #channel = "";

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
      let frame;
      try {
        frame = JSON.parse(event.data);
      } catch {
        return;
      }
      for (const handler of this.#handlers) {
        handler(frame);
      }
    });
    this.#socket.addEventListener("close", () => {
      this.#open = false;
    });
    this.#socket.addEventListener("error", () => {});
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
   * @param {object} where the broker, the secret and the instance
   */
  connectTo(where) {
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
