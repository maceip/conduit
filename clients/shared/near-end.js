/*
 * The near end of a tunnel, for any surface.
 *
 * The tunnel machinery — the keys, the sequence, the ping, the backoff,
 * the resume, the sealed payloads — is the same for every surface.  A
 * surface is a module that says what it is and how to read it:
 *
 *   {
 *     id:       "dev.conduit.browser.tabs/v1",   // the surface
 *     channel:  "tabs",                          // where it publishes
 *     schemas:  ["dev.conduit.browser.tab/v1"],  // what it may carry
 *     scopes:   ["browser.tabs.read"],           // what the user grants
 *     read:     async () => …                    // the state, or nothing
 *   }
 *
 * and this file carries it, and knows nothing about what it carries.
 * The near end never invents state: a read that returns nothing
 * publishes nothing.
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
import { tunnelUrl } from "./config.js";

export const DEFAULTS = {
  // The ping, in milliseconds.  The worker's idle timer is 30 seconds.
  pingEveryMs: 20000,

  // The backoff, in milliseconds.
  backoffMinMs: 1000,
  backoffMaxMs: 30000,
};

/* The browser's API, whichever it is. */
export function browserApi() {
  if (typeof browser !== "undefined" && browser.runtime) {
    return browser;
  }
  if (typeof chrome !== "undefined" && chrome.runtime) {
    return chrome;
  }
  return null;
}

/* The storage of the browser: Chromium and Safari both have it, under
 * two names. */
export function browserStore() {
  const api = browserApi();
  if (!api || !api.storage) {
    return null;
  }
  return api.storage.session || api.storage.local;
}

/* The Safari native host, when there is one.
 *
 * The port stays open: the near end holds the socket, the keys and the
 * sealing, and it says what happens as it happens.  The messages are
 * the near end's own messages, the same ones a plugin on a socket
 * would send. */
export function findNativeHost() {
  const api = browserApi();
  if (api && api.runtime && api.runtime.connectNative) {
    return {
      available: true,
      open() {
        const port = api.runtime.connectNative("dev.conduit.native");
        const handlers = [];
        port.onMessage.addListener((message) => {
          for (const handler of handlers) {
            handler(message);
          }
        });
        return {
          send(message) {
            port.postMessage(message);
          },
          on(handler) {
            handlers.push(handler);
          },
          close() {
            port.disconnect();
          },
        };
      },
    };
  }
  return null;
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

/* Is this socket open?  A socket that is still connecting is not. */
function isOpen(socket) {
  if (!socket) {
    return false;
  }
  if (typeof WebSocket !== "undefined" && typeof WebSocket.OPEN === "number") {
    return socket.readyState === WebSocket.OPEN;
  }
  return socket.readyState === 1;
}

/* One surface, carried over one tunnel. */
export function createNearEnd(surface, options = {}) {
  const config = {
    // The broker.  A hosted broker is wss://; a local one is ws://.
    broker: options.broker || "wss://broker.example/v1/tunnel",

    // The secret this end presents.  It comes from the device flow: the
    // user approves the extension in the broker's page, and only then
    // does the extension get one.
    secret: options.secret || "",

    // The instance of this end, for the grant.
    instance: options.instance || "extension",

    pingEveryMs: options.pingEveryMs || DEFAULTS.pingEveryMs,
    backoffMinMs: options.backoffMinMs || DEFAULTS.backoffMinMs,
    backoffMaxMs: options.backoffMaxMs || DEFAULTS.backoffMaxMs,
  };

  const store = options.store === undefined ? browserStore() : options.store;
  const nativeHost = options.nativeHost === undefined ? findNativeHost() : options.nativeHost;

  const state = {
    socket: null,
    sequence: 0,
    backoff: config.backoffMinMs,
    pingTimer: null,
    reconnectTimer: null,
    running: false,
    keys: null,
    prefix: null,
    peer: null,
    tunnel: null,
    /* The native host, when there is one: it holds the socket, the keys
     * and the sealing, and this end is only the surface. */
    native: null,
  };

  /* Read the last sequence number we saw, so that a reconnect can ask
   * for the frames it missed. */
  async function loadSequence() {
    if (!store) {
      return 0;
    }
    const key = "conduit.sequence." + surface.id;
    const held = await store.get(key);
    return held && held[key] ? held[key] : 0;
  }

  /* Keep the last sequence number we saw. */
  async function saveSequence(sequence) {
    state.sequence = sequence;
    if (!store) {
      return;
    }
    await store.set({ ["conduit.sequence." + surface.id]: sequence });
  }

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
   * The payload carries the public key of this end and the prefix of
   * its nonces: the other end needs both to open what this end seals. */
  function hello() {
    return {
      t: "hello",
      s: "dev.conduit.hello/v1",
      c: surface.channel,
      seq: nextSequence(),
      p: JSON.stringify({
        role: "extension",
        surface: surface.id,
        resume: state.sequence,
        crypto: {
          scheme: "p256-aes256gcm/v1",
          public: toBase64Url(state.keys.publicBytes),
          prefix: toBase64Url(state.prefix),
        },
      }),
    };
  }

  /* Publish the state of the surface, sealed under the key of the
   * tunnel.  Nothing is published when the read says nothing. */
  async function publish(schema, payload) {
    if (state.native) {
      /* The near end seals it; this end only says what it knows. */
      if (!state.tunnel || payload === null || payload === undefined) {
        return false;
      }
      state.native.send({ op: "publish", schema, payload });
      return true;
    }
    if (!state.tunnel || payload === null || payload === undefined) {
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
      c: surface.channel,
      seq,
      p: toBase64Url(sealed),
    });
  }

  /* Read the surface and publish what it says, under the schema that
   * was asked for when that schema is one of this surface's own. */
  async function publishState(schema) {
    const chosen = schema && surface.schemas.includes(schema) ? schema : surface.schemas[0];
    const payload = await surface.read();
    if (payload !== null && payload !== undefined) {
      await publish(chosen, payload);
    }
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

  /* Keep the connection alive, and tell the broker we are here. */
  function startPinging() {
    stopPinging();
    state.pingTimer = setInterval(() => {
      send({ t: "ping", s: "dev.conduit.ping/v1", c: surface.channel, seq: nextSequence() });
    }, config.pingEveryMs);
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
      /* Safari: the near end holds the socket, because the background
       * context will be unloaded and a loopback socket will not work.
       *
       * It is told where the broker is and what the secret is, and it
       * does the rest: the keys, the sealing, the pings. */
      const port = nativeHost.open();
      state.native = port;
      port.on(async (message) => {
        if (!message || typeof message !== "object") {
          return;
        }
        if (message.op === "status" && message.agreed) {
          state.tunnel = { native: true };
          state.backoff = config.backoffMinMs;
          /* The far end asked for the surface: say what it reads now. */
          await publishState();
          return;
        }
        if (message.op === "event") {
          /* The far end said something: the surface answers with what
           * it reads now, under the schema it asked for. */
          await publishState(message.schema);
          return;
        }
        if (message.op === "error") {
          state.tunnel = null;
          scheduleReconnect();
        }
      });
      port.send({
        op: "connect",
        broker: config.broker,
        secret: config.secret,
        instance: config.instance,
      });
      return;
    }

    /* The broker refuses an end that does not present its secret, so the
     * secret and the instance ride on the connection: the secret is the
     * user's, and the instance names this end of the grant. */
    const url = new URL(tunnelUrl(config.broker, surface.id, config.secret));
    url.searchParams.set("instance", config.instance);
    const socket = new WebSocket(url.toString());
    state.socket = socket;

    socket.onopen = () => {
      state.backoff = config.backoffMinMs;
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
        send({ t: "pong", s: "dev.conduit.pong/v1", c: surface.channel, seq: nextSequence() });
        return;
      }
      if (frame.t === "publish" && frame.p && state.tunnel) {
        const opened = await openPayload(
          state.tunnel.toClient,
          state.peer.prefix,
          frame.seq,
          fromBase64Url(frame.p),
        );
        if (!opened) {
          return;
        }
        await publishState(frame.s);
        return;
      }
      if (frame.t === "subscribe" && state.tunnel && frame.c === surface.channel) {
        /* The far end has subscribed to this channel: publish what the
         * surface reads right now, so the first frame is not a silence. */
        await publishState();
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
    state.backoff = Math.min(state.backoff * 2, config.backoffMaxMs);
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
    if (state.native) {
      state.native.close();
      state.native = null;
      state.tunnel = null;
    }
  }

  /* The start of the tunnel: with the secret the user gave, if any. */
  async function start() {
    state.sequence = await loadSequence();
    if (!state.keys) {
      const pair = await generateKeyPair();
      state.keys = pair;
      state.prefix = new Uint8Array(4);
      crypto.getRandomValues(state.prefix);
    }
    if (config.secret) {
      connect();
    }
    return config.secret !== "";
  }

  return {
    surface,
    start,
    stop,
    publish,
    publishState,
    status() {
      return {
        surface: surface.id,
        sequence: state.sequence,
        connected: state.native ? !!state.tunnel : isOpen(state.socket),
        agreed: !!state.tunnel,
      };
    },
    setSecret(secret) {
      config.secret = secret;
    },
    setBroker(broker) {
      config.broker = broker;
    },
  };
}
