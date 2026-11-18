/*
 * The conduit background: the near end of a tunnel.
 *
 * This file is the same on both browsers.  Chromium runs it in a service
 * worker; Safari runs it in a background page and hands the socket to a
 * native host, because Safari unloads idle background contexts and its
 * WebSocket to 127.0.0.1 is broken.
 *
 * The rules it follows come from the platforms, not from taste:
 *
 *   - A ping every 20 seconds.  Chromium kills a service worker after
 *     30 seconds of silence, and from Chrome 116 only traffic that is
 *     actually flowing keeps it alive.  An idle socket does not, and
 *     neither does a retry loop against a dead broker.
 *   - The last sequence number is kept, so a reconnect can ask for the
 *     frames it missed.
 *   - The backoff is exponential with jitter, so a broker that comes
 *     back is not met by a stampede.
 */

const CONFIG = {
  // The broker.  A hosted broker is wss://; a local one is ws://.
  broker: "wss://broker.example/v1/tunnel",

  // The surface this end offers, and the channel it publishes on.
  surface: "dev.conduit.mail.compose/v1",
  channel: "compose",

  // The secret this end presents.  In a shipped extension this comes
  // from the device-code flow, not from here.
  secret: "",

  // The ping, in milliseconds.  The worker's idle timer is 30 seconds.
  pingEveryMs: 20000,

  // The backoff, in milliseconds.
  backoffMinMs: 1000,
  backoffMaxMs: 30000,
};

const state = {
  socket: null,
  sequence: 0,
  backoff: CONFIG.backoffMinMs,
  pingTimer: null,
  reconnectTimer: null,
  running: false,
};

/* The storage of the browser: Chromium and Safari both have it, under
 * two names. */
const store = (function () {
  if (typeof browser !== "undefined" && browser.storage) {
    return browser.storage.session || browser.storage.local;
  }
  if (typeof chrome !== "undefined" && chrome.storage) {
    return chrome.storage.session || chrome.storage.local;
  }
  return null;
})();

/* Read the last sequence number we saw, so that a reconnect can ask for
 * the frames it missed. */
async function loadSequence() {
  if (!store) {
    return 0;
  }
  const held = await store.get("conduit.sequence");
  return held && held["conduit.sequence"] ? held["conduit.sequence"] : 0;
}

/* Keep the last sequence number we saw. */
async function saveSequence(sequence) {
  state.sequence = sequence;
  if (!store) {
    return;
  }
  await store.set({ "conduit.sequence": sequence });
}

/* The Safari native host, when there is one. */
const nativeHost = (function () {
  if (typeof browser !== "undefined" && browser.runtime && browser.runtime.connectNative) {
    return {
      available: true,
      send(message) {
        return new Promise((resolve, reject) => {
          const port = browser.runtime.connectNative("dev.conduit.native");
          port.onMessage.addListener((reply) => {
            port.disconnect();
            resolve(reply);
          });
          port.onDisconnect.addListener(() => reject(new Error("the native host went away")));
          port.postMessage(message);
        });
      },
    };
  }
  return null;
})();

/* Send one frame. */
function send(frame) {
  if (!state.socket || state.socket.readyState !== WebSocket.OPEN) {
    return false;
  }
  state.socket.send(JSON.stringify(frame));
  return true;
}

/* The hello, which says who we are and where we got to. */
function hello() {
  return {
    t: "hello",
    s: "dev.conduit.hello/v1",
    c: CONFIG.channel,
    seq: state.sequence,
    payload: JSON.stringify({
      role: "extension",
      surface: CONFIG.surface,
      resume: state.sequence,
      scopes: [],
      schemes: ["x25519-chacha20poly1305/v1"],
    }),
  };
}

/* Publish an event on the channel. */
function publish(schema, payload) {
  return send({
    t: "publish",
    s: schema,
    c: CONFIG.channel,
    seq: ++state.sequence,
    p: btoa(unescape(encodeURIComponent(JSON.stringify(payload)))),
  });
}

/* Keep the connection alive, and tell the broker we are here. */
function startPinging() {
  stopPinging();
  state.pingTimer = setInterval(() => {
    send({ t: "ping", s: "dev.conduit.ping/v1", c: CONFIG.channel, seq: ++state.sequence });
  }, CONFIG.pingEveryMs);
}

function stopPinging() {
  if (state.pingTimer !== null) {
    clearInterval(state.pingTimer);
    state.pingTimer = null;
  }
}

/* Open the tunnel. */
function connect() {
  if (state.running) {
    return;
  }
  state.running = true;

  if (nativeHost) {
    /* Safari: the native host holds the socket, because the background
     * context will be unloaded and a loopback socket will not work. */
    nativeHost
      .send({ action: "connect", url: CONFIG.broker, surface: CONFIG.surface, secret: CONFIG.secret })
      .then((reply) => {
        if (reply && reply.ok) {
          state.backoff = CONFIG.backoffMinMs;
        } else {
          scheduleReconnect();
        }
      })
      .catch(() => scheduleReconnect());
    return;
  }

  const socket = new WebSocket(CONFIG.broker + "?role=extension&surface=" + encodeURIComponent(CONFIG.surface));
  state.socket = socket;

  socket.onopen = () => {
    state.backoff = CONFIG.backoffMinMs;
    socket.send(JSON.stringify(hello()));
    startPinging();
  };

  socket.onmessage = (event) => {
    let frame;
    try {
      frame = JSON.parse(event.data);
    } catch (error) {
      return;
    }
    if (typeof frame.seq === "number" && frame.seq > state.sequence) {
      saveSequence(frame.seq);
    }
    if (frame.t === "ping") {
      send({ t: "pong", s: "dev.conduit.pong/v1", c: CONFIG.channel, seq: ++state.sequence });
    }
  };

  socket.onclose = () => {
    stopPinging();
    state.socket = null;
    scheduleReconnect();
  };

  socket.onerror = () => {
    /* onclose follows; the backoff lives there. */
  };
}

/* Come back, with a backoff that grows and is jittered. */
function scheduleReconnect() {
  if (!state.running || state.reconnectTimer !== null) {
    return;
  }
  const wait = state.backoff;
  state.backoff = Math.min(state.backoff * 2, CONFIG.backoffMaxMs);
  const jittered = wait / 2 + Math.random() * (wait / 2);
  state.reconnectTimer = setTimeout(() => {
    state.reconnectTimer = null;
    connect();
  }, jittered);
}

/* Stop the tunnel. */
function stop() {
  state.running = false;
  stopPinging();
  if (state.reconnectTimer !== null) {
    clearTimeout(state.reconnectTimer);
    state.reconnectTimer = null;
  }
  if (state.socket) {
    state.socket.close();
    state.socket = null;
  }
}

/* The message that arrives from the page or from the native side. */
async function onMessage(message) {
  if (!message || !message.action) {
    return;
  }
  switch (message.action) {
    case "publish":
      publish(message.schema || "dev.conduit.generic/v1", message.payload);
      break;
    case "status":
      return { ok: true, sequence: state.sequence, connected: !!state.socket };
    case "toggle":
      if (state.socket) {
        stop();
      } else {
        start();
      }
      break;
    case "stop":
      stop();
      break;
    default:
      break;
  }
  return { ok: true };
}

/* The start of the tunnel. */
async function start() {
  state.sequence = await loadSequence();
  connect();
}

/* Wire the messages, in whichever browser this is. */
if (typeof browser !== "undefined" && browser.runtime && browser.runtime.onMessage) {
  browser.runtime.onMessage.addListener(onMessage);
  browser.runtime.onInstalled.addListener(start);
  browser.runtime.onStartup.addListener(start);
} else if (typeof chrome !== "undefined" && chrome.runtime) {
  chrome.runtime.onMessage.addListener(onMessage);
  chrome.runtime.onInstalled.addListener(start);
  chrome.runtime.onStartup.addListener(start);
}

/* In a service worker, the worker may be started by a message rather
 * than by an event, so start here as well. */
start();

export { publish, onMessage, start, stop };
