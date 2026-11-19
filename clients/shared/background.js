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

import { agree, fromBase64Url, generateKeyPair, open as openPayload, seal, toBase64Url } from "./sealing.js";
import { CHANNEL, SCHEMA, SCOPE, readTab } from "./surface-browser.js";
import { DEFAULT_BROKER, endpoint, parseBroker, tunnelUrl } from "./config.js";

const CONFIG = {
  // The broker.  A hosted broker is wss://; a local one is ws://.  The
  // user sets it in the popup; it lives in storage as conduit.broker.
  broker: DEFAULT_BROKER,

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

  // How often to ask whether the user approved the code.
  approvalPollMs: 2000,

  // The backoff, in milliseconds.
  backoffMinMs: 1000,
  backoffMaxMs: 30000,
};

const state = {
  socket: null,
  welcomed: false,
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
  pendingExpires: 0,
  approvalTimer: null,
  starting: null,
  inbox: Promise.resolve(),
  lastError: null,
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

/* Two stores.  What has to outlive the browser (the broker, the secret,
 * whether the user stopped the tunnel) goes in local storage; what only
 * matters while the browser runs (the sequence number, a pending code)
 * goes in session storage where there is one. */
const durable = (function () {
  const api = browserApi;
  return api && api.storage && api.storage.local ? api.storage.local : null;
})();

const store = (function () {
  const api = browserApi;
  if (!api || !api.storage) {
    return null;
  }
  return api.storage.session || api.storage.local;
})();

async function read(where, key, fallback) {
  if (!where) {
    return fallback;
  }
  const held = await where.get(key);
  return held && held[key] !== undefined && held[key] !== null ? held[key] : fallback;
}

async function write(where, values) {
  if (where) {
    await where.set(values);
  }
}

async function forget(where, keys) {
  if (where) {
    await where.remove(keys);
  }
}

/* Keep the last sequence number we saw. */
async function saveSequence(sequence) {
  state.sequence = sequence;
  await write(store, { "conduit.sequence": sequence });
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
          port.onDisconnect.addListener(() => reject(new Error("native host disconnected")));
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
  const resume = state.sequence;
  return {
    t: "hello",
    s: "dev.conduit.hello/v1",
    c: CONFIG.channel,
    seq: nextSequence(),
    p: JSON.stringify({
      role: "extension",
      surface: CONFIG.surface,
      resume,
      crypto: {
        scheme: "p256-aes256gcm/v1",
        public: toBase64Url(state.keys.publicBytes),
        prefix: toBase64Url(state.prefix),
      },
    }),
  };
}

/* Publish an event on the channel, sealed under the key of the
 * tunnel.  The nonce is this end's prefix + the frame's seq: the
 * client opens it with the prefix this end sent in its hello. */
async function publish(schema, payload) {
  if (!state.tunnel) {
    return false;
  }
  const seq = nextSequence();
  const sealed = await seal(state.tunnel.toExtension, state.prefix, seq, JSON.stringify(payload));
  return send({
    t: "publish",
    s: schema,
    c: CONFIG.channel,
    seq,
    p: toBase64Url(sealed),
  });
}

/* Publish the tab in front of the user, if a client is there to open
 * it. */
async function publishActiveTab() {
  if (!state.tunnel) {
    return false;
  }
  const tab = await readTab();
  if (!tab) {
    return false;
  }
  return publish(SCHEMA, tab);
}

/* Take the hello of the other end and agree the keys of the tunnel.
 * A new client means a new hello => new keys. */
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

/* One frame from the broker.  Frames are taken one at a time, in
 * order: the hello of a client has to be agreed before the subscribe
 * that follows it can be answered. */
async function onFrame(frame) {
  if (typeof frame.seq === "number" && frame.seq > state.sequence) {
    await saveSequence(frame.seq);
  }
  switch (frame.t) {
    case "welcome":
      state.welcomed = true;
      state.lastError = null;
      return;
    case "hello":
      if (frame.p) {
        await meet(frame.p);
      }
      return;
    case "subscribe":
      /* A client subscribed: give it the tab in front of the user now,
       * rather than at the next change. */
      await publishActiveTab();
      return;
    case "ping":
      send({ t: "pong", s: "dev.conduit.pong/v1", c: CONFIG.channel, seq: nextSequence() });
      return;
    case "error": {
      let error = {};
      try {
        error = JSON.parse(frame.p || "{}");
      } catch (problem) {
        error = {};
      }
      state.lastError = error.code || "error";
      if (error.code === "unauthorized") {
        /* The broker doesn't know the secret (it restarted, or the user
         * took it away) => it's no good; pair again. */
        await forgetTheSecret();
        stop();
        await start();
      }
      return;
    }
    case "publish": {
      if (!frame.p || !state.tunnel) {
        return;
      }
      let opened;
      try {
        opened = await openPayload(state.tunnel.toClient, state.peer.prefix, frame.seq, fromBase64Url(frame.p));
      } catch (problem) {
        return;
      }
      const payload = JSON.parse(new TextDecoder().decode(opened));
      await onEvent(frame.s, payload);
      return;
    }
    default:
      return;
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

  const socket = new WebSocket(tunnelUrl(CONFIG.broker, CONFIG.surface, CONFIG.secret));
  state.socket = socket;
  state.welcomed = false;
  state.tunnel = null;
  state.peer = null;

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
    state.inbox = state.inbox.then(() => onFrame(frame)).catch(() => {});
  };

  socket.onclose = () => {
    if (state.socket !== socket) {
      return;
    }
    stopPinging();
    state.socket = null;
    state.welcomed = false;
    state.tunnel = null;
    state.peer = null;
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
    state.running = false;
    connect();
  }, jittered);
}

/* Stop the tunnel, and any pairing in flight. */
function stop() {
  state.running = false;
  stopPinging();
  if (state.reconnectTimer !== null) {
    clearTimeout(state.reconnectTimer);
    state.reconnectTimer = null;
  }
  if (state.approvalTimer !== null) {
    clearInterval(state.approvalTimer);
    state.approvalTimer = null;
  }
  const socket = state.socket;
  state.socket = null;
  state.welcomed = false;
  state.tunnel = null;
  state.peer = null;
  if (socket) {
    socket.close();
  }
}

/* What comes in from the other end. */
async function onEvent(schema, payload) {
  if (schema === SCHEMA) {
    await publish(SCHEMA, payload);
  }
}

/* What the popup sees. */
function status() {
  return {
    ok: true,
    broker: CONFIG.broker,
    sequence: state.sequence,
    paired: !!CONFIG.secret,
    pairing: !!state.pendingCode,
    running: state.running || state.approvalTimer !== null,
    connected: !!state.socket && state.socket.readyState === WebSocket.OPEN && state.welcomed,
    agreed: !!state.tunnel,
    error: state.lastError,
  };
}

/* The message that arrives from the page or from the native side. */
async function onMessage(message) {
  if (!message || !message.action) {
    return;
  }
  switch (message.action) {
    case "publish":
      /* The surface: the tab in front of the user. */
      await publishActiveTab();
      break;
    case "status":
      await state.starting;
      return status();
    case "toggle":
      if (state.running || state.approvalTimer !== null) {
        await write(durable, { "conduit.stopped": true });
        stop();
      } else {
        await write(durable, { "conduit.stopped": false });
        await start();
      }
      return status();
    case "stop":
      await write(durable, { "conduit.stopped": true });
      stop();
      return status();
    case "configure": {
      const broker = parseBroker(message.broker);
      if (!broker) {
        return { ok: false, error: "not a ws:// or wss:// address" };
      }
      if (broker !== CONFIG.broker) {
        /* A secret belongs to the broker that gave it out. */
        stop();
        await write(durable, { "conduit.broker": broker, "conduit.stopped": false });
        await forgetTheSecret();
        CONFIG.broker = broker;
        await start();
      }
      return status();
    }
    default:
      break;
  }
  return { ok: true };
}

/* Answer through sendResponse and return `true`: every browser takes
 * that, while a returned promise is dropped by the Chromium versions
 * this extension supports. */
function listen(message, sender, sendResponse) {
  onMessage(message).then(
    (reply) => sendResponse(reply),
    (error) => sendResponse({ ok: false, error: String(error) }),
  );
  return true;
}

async function forgetTheSecret() {
  CONFIG.secret = "";
  state.pendingCode = null;
  await forget(durable, ["conduit.secret"]);
  await forget(store, ["conduit.pending"]);
}

/* Ask the broker for a code, and open the page where the user
 * approves it.
 *
 * The broker does not hand out a secret because an extension asked:
 * the user says yes first, in the broker's own page. */
async function askForApproval() {
  const api = browserApi;
  const answer = await fetch(endpoint(CONFIG.broker, "/v1/device/code"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      surface: CONFIG.surface,
      scopes: [SCOPE],
    }),
  });
  if (!answer.ok) {
    throw new Error(`the broker said ${answer.status} to the code`);
  }
  const code = await answer.json();
  state.pendingCode = code.code;
  state.pendingExpires = Date.now() + code.expires_in * 1000;
  await write(store, {
    "conduit.pending": { code: code.code, expires: state.pendingExpires, broker: CONFIG.broker },
  });
  if (api && api.tabs && api.tabs.create) {
    await api.tabs.create({ url: endpoint(CONFIG.broker, code.url) });
  }
}

/* Ask the broker whether the user has approved the code. */
async function pollForTheSecret() {
  if (!state.pendingCode) {
    return false;
  }
  const answer = await fetch(endpoint(CONFIG.broker, "/v1/device/secret"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ code: state.pendingCode, instance: CONFIG.instance }),
  });
  if (!answer.ok) {
    return false;
  }
  const secret = await answer.text();
  CONFIG.secret = secret;
  state.pendingCode = null;
  await write(durable, { "conduit.secret": secret });
  await forget(store, ["conduit.pending"]);
  return true;
}

/* Wait for the user to approve the code, then open the tunnel. */
function waitForApproval() {
  if (state.approvalTimer !== null) {
    clearInterval(state.approvalTimer);
  }
  let asking = false;
  state.approvalTimer = setInterval(async () => {
    if (asking) {
      return;
    }
    asking = true;
    try {
      if (Date.now() > state.pendingExpires) {
        /* The code died before the user approved it: ask for another. */
        clearInterval(state.approvalTimer);
        state.approvalTimer = null;
        await forget(store, ["conduit.pending"]);
        state.pendingCode = null;
        await start();
        return;
      }
      if (await pollForTheSecret()) {
        clearInterval(state.approvalTimer);
        state.approvalTimer = null;
        connect();
      }
    } catch (error) {
      state.lastError = String(error && error.message ? error.message : error);
    } finally {
      asking = false;
    }
  }, CONFIG.approvalPollMs);
}

/* The start of the tunnel.  One start at a time: the worker starts on
 * install and on its own load, and two starts would ask for two codes
 * and open two approval tabs. */
function start() {
  if (!state.starting) {
    state.starting = begin().finally(() => {
      state.starting = null;
    });
  }
  return state.starting;
}

async function begin() {
  CONFIG.broker = parseBroker(await read(durable, "conduit.broker", DEFAULT_BROKER)) || DEFAULT_BROKER;
  state.sequence = await read(store, "conduit.sequence", 0);
  if (!state.keys) {
    const pair = await generateKeyPair();
    state.keys = pair;
    state.prefix = new Uint8Array(4);
    crypto.getRandomValues(state.prefix);
  }
  if (await read(durable, "conduit.stopped", false)) {
    return;
  }
  const secret = await read(durable, "conduit.secret", "");
  if (secret) {
    CONFIG.secret = secret;
    connect();
    return;
  }
  if (state.approvalTimer !== null) {
    return;
  }
  // There is no secret yet: ask for one, and wait for the user.  A code
  // asked for before the worker was unloaded is still good.
  const pending = await read(store, "conduit.pending", null);
  try {
    if (pending && pending.broker === CONFIG.broker && pending.expires > Date.now()) {
      state.pendingCode = pending.code;
      state.pendingExpires = pending.expires;
    } else {
      await askForApproval();
    }
  } catch (error) {
    state.lastError = String(error && error.message ? error.message : error);
    return;
  }
  waitForApproval();
}

/* Wire the messages and the tabs, in whichever browser this is.  A
 * service worker has to add its listeners at the top level, on every
 * load. */
if (browserApi && browserApi.runtime && browserApi.runtime.onMessage) {
  browserApi.runtime.onMessage.addListener(listen);
  if (browserApi.runtime.onInstalled) {
    browserApi.runtime.onInstalled.addListener(() => start());
  }
  if (browserApi.runtime.onStartup) {
    browserApi.runtime.onStartup.addListener(() => start());
  }
}
if (browserApi && browserApi.tabs) {
  if (browserApi.tabs.onActivated) {
    browserApi.tabs.onActivated.addListener(() => publishActiveTab());
  }
  if (browserApi.tabs.onUpdated) {
    browserApi.tabs.onUpdated.addListener((tabId, change, tab) => {
      if (tab && tab.active && (change.status === "complete" || change.title !== undefined)) {
        publishActiveTab();
      }
    });
  }
}

/* In a service worker, the worker may be started by a message rather
 * than by an event, so start here as well. */
start();

export { publish, onMessage, start, stop };
