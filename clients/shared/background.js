/*
 * The conduit background: the near end of a tunnel.
 *
 * This file is the same on both browsers.  Chromium runs it in a
 * service worker; Safari runs it in a background page and hands the
 * socket to a native host, because Safari unloads idle background
 * contexts and its WebSocket to 127.0.0.1 is broken.
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
 *   - The payload is sealed between the two ends.  The broker carries
 *     it and never opens it.
 *
 * Every browser-only call is guarded: Safari throws on a Chromium-only
 * call at the top level of a background script, and then discards the
 * background context without a word.
 */

import { agree, generateKeyPair, open as openPayload, seal, toBase64Url } from "./sealing.js";
import { CHANNEL, SCHEMA, SCOPE, readTab } from "./surface-browser.js";

const CONFIG = {
  // The broker.  A hosted broker is wss://; a local one is ws://.
  broker: "wss://broker.example/v1/tunnel",

  // The surface this end offers, and the channel it publishes on.
  surface: "dev.conduit.browser.tabs/v1",
  channel: CHANNEL,

  // The secret this end presents.  It comes from the device flow: the
  // user approves the extension in the broker's page, and only then
  // does the extension get one.
  secret: "",

  // The instance of this end, for the grant.
  instance: "extension",

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
  keys: null,
  prefix: null,
  peer: null,
  tunnel: null,
  pendingCode: null,
  approvalTimer: null,
};

/* The browser's API, whichever it is. */
const browserApi = (function () {
  if (typeof browser !== "undefined" && browser.runtime) {
    return browser;
  }
  if (typeof chrome !== "undefined" && chrome.runtime) {
    return chrome;
  }
  return null;
})();

/* The storage of the browser: Chromium and Safari both have it, under
 * two names. */
const store = (function () {
  const api = browserApi;
  if (!api || !api.storage) {
    return null;
  }
  return api.storage.session || api.storage.local;
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
  const api = browserApi;
  if (api && api.runtime && api.runtime.connectNative) {
    return {
      available: true,
      send(message) {
        return new Promise((resolve, reject) => {
          const port = api.runtime.connectNative("dev.conduit.native");
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

/* The next sequence number of this end.
 *
 * The broker keeps the number of the end that sent the frame, because
 * that number is part of the nonce the payload is sealed under. */
function nextSequence() {
  state.sequence += 1;
  return state.sequence;
}

/* Send one frame. */
function send(frame) {
  if (!state.socket || state.socket.readyState !== WebSocket.OPEN) {
    return false;
  }
  state.socket.send(JSON.stringify(frame));
  return true;
}

/* The hello, which says who we are and where we got to.
 *
 * The payload carries the public key of this end and the prefix of its
 * nonces: the other end needs both to open what this end seals. */
function hello() {
  return {
    t: "hello",
    s: "dev.conduit.hello/v1",
    c: CONFIG.channel,
    seq: nextSequence(),
    p: JSON.stringify({
      role: "extension",
      surface: CONFIG.surface,
      resume: state.sequence,
      crypto: {
        scheme: "p256-aes256gcm/v1",
        public: toBase64Url(state.keys.publicBytes),
        prefix: toBase64Url(state.prefix),
      },
    }),
  };
}

/* Publish an event on the channel, sealed under the key of the
 * tunnel. */
async function publish(schema, payload) {
  if (!state.tunnel) {
    return false;
  }
  const seq = nextSequence();
  /* The nonce prefix of a frame is the one of the end that sent it:
   * the hello carries each end's own prefix, and the far end opens
   * this frame with this end's prefix. */
  const sealed = await seal(state.tunnel.toExtension, state.prefix, seq, JSON.stringify(payload));
  return send({
    t: "publish",
    s: schema,
    c: CONFIG.channel,
    seq,
    p: toBase64Url(sealed),
  });
}

/* Take the hello of the other end and agree the keys of the tunnel. */
async function meet(payload) {
  const hello = JSON.parse(payload);
  const crypto = hello.crypto || {};
  if (crypto.scheme !== "p256-aes256gcm/v1") {
    return false;
  }
  const peerPublic = fromBase64Url(crypto.public);
  const peerPrefix = fromBase64Url(crypto.prefix);
  const shared = await agree(state.keys.privateKey, peerPublic);
  state.peer = { prefix: peerPrefix, public: peerPublic };
  state.tunnel = {
    toExtension: shared.toExtension,
    toClient: shared.toClient,
  };
  return true;
}

/* Base64url without padding, back to bytes. */
function fromBase64Url(text) {
  const padded = text.replace(/-/g, "+").replace(/_/g, "/");
  const binary = atob(padded + "=".repeat((4 - (padded.length % 4)) % 4));
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

/* Keep the connection alive, and tell the broker we are here. */
function startPinging() {
  stopPinging();
  state.pingTimer = setInterval(() => {
    send({ t: "ping", s: "dev.conduit.ping/v1", c: CONFIG.channel, seq: nextSequence() });
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
      .send({
        action: "connect",
        url: CONFIG.broker,
        surface: CONFIG.surface,
        secret: CONFIG.secret,
        crypto: {
          scheme: "p256-aes256gcm/v1",
          public: toBase64Url(state.keys.publicBytes),
          prefix: toBase64Url(state.prefix),
        },
      })
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

  /* The broker refuses an end that does not present its secret, so the
   * secret and the instance ride on the connection: the secret is the
   * user's, and the instance names this end of the grant. */
  const socket = new WebSocket(
    CONFIG.broker +
      "?role=extension&surface=" +
      encodeURIComponent(CONFIG.surface) +
      "&secret=" +
      encodeURIComponent(CONFIG.secret) +
      "&instance=" +
      encodeURIComponent(CONFIG.instance),
  );
  state.socket = socket;

  socket.onopen = () => {
    state.backoff = CONFIG.backoffMinMs;
    socket.send(JSON.stringify(hello()));
    startPinging();
  };

  socket.onmessage = async (event) => {
    let frame;
    try {
      frame = JSON.parse(event.data);
    } catch (error) {
      return;
    }
    if (typeof frame.seq === "number" && frame.seq > state.sequence) {
      saveSequence(frame.seq);
    }
    if (frame.t === "hello" && frame.p) {
      await meet(frame.p);
      return;
    }
    if (frame.t === "ping") {
      send({ t: "pong", s: "dev.conduit.pong/v1", c: CONFIG.channel, seq: nextSequence() });
      return;
    }
    if (frame.t === "publish" && frame.p && state.tunnel) {
      const opened = await openPayload(state.tunnel.toClient, state.peer.prefix, frame.seq, fromBase64Url(frame.p));
      if (!opened) {
        return;
      }
      await onEvent(frame.s);
      return;
    }
    if (frame.t === "subscribe" && state.tunnel && frame.c === CONFIG.channel) {
      /* The far end has subscribed to this channel: publish what the
       * surface reads right now, so the first frame is not a silence. */
      await onEvent(SCHEMA);
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

/* What comes in from the other end.
 *
 * A frame that names the tab schema is a request for the tab in front
 * of the user.  The answer is what the surface reads — the tab itself,
 * or nothing at all when there is none.  A surface never answers with a
 * stand-in. */
async function onEvent(schema) {
  if (schema === SCHEMA) {
    const tab = await readTab();
    if (tab) {
      await publish(SCHEMA, tab);
    }
  }
}

/* The message that arrives from the page or from the native side. */
async function onMessage(message) {
  if (!message || !message.action) {
    return;
  }
  switch (message.action) {
    case "publish": {
      /* The surface: the tab in front of the user. */
      const tab = await readTab();
      if (tab) {
        await publish(SCHEMA, tab);
      }
      break;
    }
    case "status":
      return {
        ok: true,
        sequence: state.sequence,
        connected: !!state.socket,
        agreed: !!state.tunnel,
      };
    case "toggle":
      if (state.socket) {
        stop();
      } else {
        await start();
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

/* The broker this end talks to, if the user has pointed it at one.
 *
 * The default is the one in CONFIG; the user may hold a different one
 * (a local broker, say), and that is their choice to make and to take
 * away. */
async function loadBroker() {
  if (!store) {
    return CONFIG.broker;
  }
  const held = await store.get("conduit.broker");
  return held && held["conduit.broker"] ? held["conduit.broker"] : CONFIG.broker;
}

/* The secret, if the user has approved one. */
async function loadSecret() {
  if (!store) {
    return "";
  }
  const held = await store.get("conduit.secret");
  return held && held["conduit.secret"] ? held["conduit.secret"] : "";
}

/* Keep the secret.
 *
 * The secret is the user's decision: it comes from the device flow,
 * and it is not the extension's to keep if the user takes it away. */
async function saveSecret(secret) {
  if (!store) {
    return;
  }
  await store.set({ "conduit.secret": secret });
}

/* Ask the broker for a code, and open the page where the user
 * approves it.
 *
 * The broker does not hand out a secret because an extension asked:
 * the user says yes first, in the broker's own page. */
async function askForApproval() {
  const api = browserApi;
  if (!api || !api.runtime || !api.runtime.sendMessage) {
    return;
  }
  const answer = await fetch(CONFIG.broker.replace(/^ws/, "http") + "/v1/device/code", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      surface: CONFIG.surface,
      scopes: [SCOPE],
    }),
  });
  if (!answer.ok) {
    return;
  }
  const code = await answer.json();
  const page = CONFIG.broker.replace(/^ws/, "http") + code.url;
  if (api.tabs && api.tabs.create) {
    await api.tabs.create({ url: page });
  }
  state.pendingCode = code.code;
}

/* Ask the broker whether the user has approved the code. */
async function pollForTheSecret() {
  if (!state.pendingCode) {
    return false;
  }
  const answer = await fetch(CONFIG.broker.replace(/^ws/, "http") + "/v1/device/secret", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ code: state.pendingCode, instance: CONFIG.instance }),
  });
  if (!answer.ok) {
    return false;
  }
  const secret = await answer.text();
  await saveSecret(secret);
  state.pendingCode = null;
  CONFIG.secret = secret;
  return true;
}

/* The start of the tunnel. */
async function start() {
  state.sequence = await loadSequence();
  CONFIG.broker = await loadBroker();
  if (!state.keys) {
    const pair = await generateKeyPair();
    state.keys = pair;
    state.prefix = new Uint8Array(4);
    crypto.getRandomValues(state.prefix);
  }
  const secret = await loadSecret();
  if (secret) {
    CONFIG.secret = secret;
    connect();
    return;
  }
  // There is no secret yet: ask for one, and wait for the user.
  await askForApproval();
  if (state.approvalTimer !== null) {
    clearInterval(state.approvalTimer);
  }
  state.approvalTimer = setInterval(async () => {
    if (await pollForTheSecret()) {
      clearInterval(state.approvalTimer);
      state.approvalTimer = null;
      connect();
    }
  }, 5000);
}

/* Wire the messages, in whichever browser this is. */
if (browserApi && browserApi.runtime && browserApi.runtime.onMessage) {
  browserApi.runtime.onMessage.addListener(onMessage);
  if (browserApi.runtime.onInstalled) {
    browserApi.runtime.onInstalled.addListener(start);
  }
  if (browserApi.runtime.onStartup) {
    browserApi.runtime.onStartup.addListener(start);
  }
}

/* In a service worker, the worker may be started by a message rather
 * than by an event, so start here as well. */
start();

export { publish, onMessage, start, stop };
