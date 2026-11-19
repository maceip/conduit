/*
 * The extension: the surfaces it offers, the device flow that gets the
 * user's secret, and the popup's questions.
 *
 * The tunnel machinery lives in near-end.js, and it is the same for
 * every surface — that is the point of it.  What lives here is what
 * differs: which surfaces the extension offers, which secret belongs
 * to which surface, and what the user is told.
 *
 * A surface never publishes a stand-in: when the read says there is
 * nothing, nothing is published.
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

import { browserApi, createNearEnd } from "./near-end.js";
import { CHANNEL, SCHEMA, SCOPE, readTab } from "./surface-browser.js";
import {
  CHANNEL as WINDOWS_CHANNEL,
  SCHEMA as WINDOWS_SCHEMA,
  SCOPE as WINDOWS_SCOPE,
  readWindow,
} from "./surface-windows.js";
import { DEFAULT_BROKER, endpoint, parseBroker } from "./config.js";

const CONFIG = {
  // The broker.  A hosted broker is wss://; a local one is ws://.  The
  // user sets it in the popup; it lives in storage as conduit.broker.
  broker: DEFAULT_BROKER,

  // The instance of this end, for the grant.  The broker binds the
  // instance to the secret when it gives the secret out, so an
  // extension cannot claim another instance's grant by naming it.
  instance: "extension",

  // How often to ask whether the user approved the code.
  approvalPollMs: 2000,
};

/* The surfaces this extension offers.  Each is a module: what it is,
 * where it publishes, what the user grants, and how to read it. */
const SURFACES = [
  {
    id: "dev.conduit.browser.tabs/v1",
    channel: CHANNEL,
    schemas: [SCHEMA],
    scopes: [SCOPE],
    read: readTab,
  },
  {
    id: "dev.conduit.browser.windows/v1",
    channel: WINDOWS_CHANNEL,
    schemas: [WINDOWS_SCHEMA],
    scopes: [WINDOWS_SCOPE],
    read: readWindow,
  },
];

const api = browserApi();

/* Two stores.  What has to outlive the browser (the broker, the
 * secrets, whether the user stopped the tunnel) goes in local storage;
 * what only matters while the browser runs (the sequence numbers, a
 * pending code) goes in session storage where there is one. */
const durable = api && api.storage && api.storage.local ? api.storage.local : null;
const store = api && api.storage ? api.storage.session || api.storage.local : null;

const state = {
  broker: DEFAULT_BROKER,
  stopped: false,
  starting: null,
  lastError: null,
  /* One entry per surface: the near end that carries it, the secret it
   * holds, and the code the user is yet to approve. */
  surfaces: new Map(),
};

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

/* The key a surface's secret is kept under.  A secret belongs to one
 * surface — the broker binds it — so the keys are per surface. */
function secretKey(surfaceId) {
  return "conduit.secret." + surfaceId;
}

function pendingKey(surfaceId) {
  return "conduit.pending." + surfaceId;
}

/* Build the near end of every surface.  They are built once and kept;
 * the broker and the secret can change underneath them. */
for (const surface of SURFACES) {
  const entry = {
    surface,
    end: null,
    secret: "",
    pending: null,
    approvalTimer: null,
    asking: false,
  };
  entry.end = createNearEnd(surface, {
    broker: state.broker,
    instance: CONFIG.instance,
    store: store,
  });
  state.surfaces.set(surface.id, entry);
}

/* The entry of a surface. */
function entryOf(surfaceId) {
  return state.surfaces.get(surfaceId) || null;
}

/* Ask the broker for a code, and open the page where the user
 * approves it.
 *
 * The broker does not hand out a secret because an extension asked:
 * the user says yes first, in the broker's own page.  A code is asked
 * for per surface, because the secret it becomes belongs to that
 * surface and no other. */
async function askForApproval(entry) {
  const answer = await fetch(endpoint(state.broker, "/v1/device/code"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      surface: entry.surface.id,
      scopes: entry.surface.scopes,
    }),
  });
  if (!answer.ok) {
    throw new Error(`the broker said ${answer.status} to the code`);
  }
  const code = await answer.json();
  entry.pending = {
    code: code.code,
    expires: Date.now() + code.expires_in * 1000,
    broker: state.broker,
  };
  await write(store, { [pendingKey(entry.surface.id)]: entry.pending });
  if (api && api.tabs && api.tabs.create) {
    await api.tabs.create({ url: endpoint(state.broker, code.url) });
  }
}

/* Ask the broker whether the user has approved the code. */
async function pollForTheSecret(entry) {
  if (!entry.pending) {
    return false;
  }
  const answer = await fetch(endpoint(state.broker, "/v1/device/secret"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ code: entry.pending.code, instance: CONFIG.instance }),
  });
  if (!answer.ok) {
    return false;
  }
  const secret = await answer.text();
  entry.secret = secret;
  entry.pending = null;
  entry.end.setSecret(secret);
  await write(durable, { [secretKey(entry.surface.id)]: secret });
  await forget(store, [pendingKey(entry.surface.id)]);
  return true;
}

/* Wait for the user to approve the code, then open the tunnel. */
function waitForApproval(entry) {
  if (entry.approvalTimer !== null) {
    clearInterval(entry.approvalTimer);
  }
  entry.approvalTimer = setInterval(async () => {
    if (entry.asking) {
      return;
    }
    entry.asking = true;
    try {
      if (!entry.pending || Date.now() > entry.pending.expires) {
        /* The code died before the user approved it: ask for another. */
        clearInterval(entry.approvalTimer);
        entry.approvalTimer = null;
        entry.pending = null;
        await forget(store, [pendingKey(entry.surface.id)]);
        await start();
        return;
      }
      if (await pollForTheSecret(entry)) {
        clearInterval(entry.approvalTimer);
        entry.approvalTimer = null;
        entry.end.start();
      }
    } catch (error) {
      state.lastError = String(error && error.message ? error.message : error);
    } finally {
      entry.asking = false;
    }
  }, CONFIG.approvalPollMs);
}

/* The secret of a surface, if the user has approved one. */
async function loadSecret(entry) {
  const secret = await read(durable, secretKey(entry.surface.id), "");
  entry.secret = secret || "";
  entry.end.setSecret(entry.secret);
  return entry.secret;
}

/* Forget the secret of a surface.  The user took it away, or the
 * broker changed. */
async function forgetTheSecret(entry) {
  entry.secret = "";
  entry.pending = null;
  entry.end.setSecret("");
  await forget(durable, [secretKey(entry.surface.id)]);
  await forget(store, [pendingKey(entry.surface.id)]);
}

/* Publish what every surface reads right now.  A surface that has
 * nothing to say says nothing. */
async function publishEverywhere() {
  for (const entry of state.surfaces.values()) {
    await entry.end.publishState();
  }
}

/* What the popup sees.
 *
 * The one line is the state of the tunnel as a whole; the surfaces
 * are listed under it, because a surface that is up and a surface that
 * is not are different facts. */
function status() {
  const surfaces = [];
  let paired = 0;
  let connecting = 0;
  let pairing = false;
  for (const entry of state.surfaces.values()) {
    const near = entry.end.status();
    surfaces.push({
      id: entry.surface.id,
      paired: !!entry.secret,
      pairing: !!entry.pending,
      connected: near.connected,
      agreed: near.agreed,
    });
    if (entry.secret) {
      paired += 1;
      if (near.connected) {
        connecting += 1;
      }
    }
    if (entry.pending) {
      pairing = true;
    }
  }
  const total = state.surfaces.size;
  /* Every surface the extension offers has to be up before the tunnel
   * is up: half a tunnel is not a tunnel. */
  const allPaired = paired === total;
  const allConnected = connecting === total;
  return {
    ok: true,
    broker: state.broker,
    paired: allPaired,
    pairing,
    running: !state.stopped && (allConnected || pairing),
    connected: allConnected,
    agreed: connecting > 0,
    stopped: state.stopped,
    surfaces,
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
      /* The surfaces: what the browser knows right now. */
      await publishEverywhere();
      break;
    case "status":
      await state.starting;
      return status();
    case "toggle": {
      const now = status();
      if (now.running || now.pairing) {
        state.stopped = true;
        await write(durable, { "conduit.stopped": true });
        stop();
      } else {
        state.stopped = false;
        await write(durable, { "conduit.stopped": false });
        await start();
      }
      return status();
    }
    case "stop":
      state.stopped = true;
      await write(durable, { "conduit.stopped": true });
      stop();
      return status();
    case "configure": {
      const broker = parseBroker(message.broker);
      if (!broker) {
        return { ok: false, error: "not a ws:// or wss:// address" };
      }
      if (broker !== state.broker) {
        /* A secret belongs to the broker that gave it out. */
        stop();
        await write(durable, { "conduit.broker": broker, "conduit.stopped": false });
        state.broker = broker;
        for (const entry of state.surfaces.values()) {
          entry.end.setBroker(broker);
          await forgetTheSecret(entry);
        }
        state.stopped = false;
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

/* Stop the tunnel, and any pairing in flight. */
function stop() {
  for (const entry of state.surfaces.values()) {
    if (entry.approvalTimer !== null) {
      clearInterval(entry.approvalTimer);
      entry.approvalTimer = null;
    }
    entry.pending = null;
    entry.end.stop();
  }
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
  state.broker =
    parseBroker(await read(durable, "conduit.broker", DEFAULT_BROKER)) || DEFAULT_BROKER;
  state.stopped = !!(await read(durable, "conduit.stopped", false));
  for (const entry of state.surfaces.values()) {
    entry.end.setBroker(state.broker);
  }
  if (state.stopped) {
    return;
  }
  for (const entry of state.surfaces.values()) {
    const secret = await loadSecret(entry);
    if (secret) {
      await entry.end.start();
      continue;
    }
    if (entry.approvalTimer !== null) {
      continue;
    }
    /* There is no secret yet: ask for one, and wait for the user.  A
     * code asked for before the worker was unloaded is still good. */
    const pending = await read(store, pendingKey(entry.surface.id), null);
    try {
      if (pending && pending.broker === state.broker && pending.expires > Date.now()) {
        entry.pending = pending;
      } else {
        await askForApproval(entry);
      }
    } catch (error) {
      state.lastError = String(error && error.message ? error.message : error);
      continue;
    }
    waitForApproval(entry);
  }
}

/* Wire the messages and the tabs, in whichever browser this is.  A
 * service worker has to add its listeners at the top level, on every
 * load. */
if (api && api.runtime && api.runtime.onMessage) {
  api.runtime.onMessage.addListener(listen);
  if (api.runtime.onInstalled) {
    api.runtime.onInstalled.addListener(() => start());
  }
  if (api.runtime.onStartup) {
    api.runtime.onStartup.addListener(() => start());
  }
}
if (api && api.tabs) {
  const tabSurface = entryOf("dev.conduit.browser.tabs/v1");
  const publishTabs = () => {
    if (tabSurface) {
      tabSurface.end.publishState();
    }
  };
  if (api.tabs.onActivated) {
    api.tabs.onActivated.addListener(publishTabs);
  }
  if (api.tabs.onUpdated) {
    api.tabs.onUpdated.addListener((tabId, change, tab) => {
      if (tab && tab.active && (change.status === "complete" || change.title !== undefined)) {
        publishTabs();
      }
    });
  }
}

/* In a service worker, the worker may be started by a message rather
 * than by an event, so start here as well. */
start();

export { onMessage, start, stop, status };
