/*
 * The conduit background: the near ends of the tunnels.
 *
 * This file is the same on both browsers.  Chromium runs it in a
 * service worker; Safari runs it in a background page and hands the
 * socket to a native host, because Safari unloads idle background
 * contexts and its WebSocket to 127.0.0.1 is broken.
 *
 * What lives here is the extension: the surfaces it offers, the
 * user's secret and broker, and the device flow that gets the secret
 * from the user.  The tunnel machinery lives in near-end.js, and it is
 * the same for every surface — that is the point of it.
 *
 * A surface never publishes a stand-in: when the read says there is
 * nothing, nothing is published.
 */

import { browserStore, createNearEnd } from "./near-end.js";
import { CHANNEL, SCOPE, readTab, SCHEMA } from "./surface-browser.js";
import { CHANNEL as WINDOWS_CHANNEL, SCOPE as WINDOWS_SCOPE, readWindow, SCHEMA as WINDOWS_SCHEMA } from "./surface-windows.js";

const CONFIG = {
  // The broker.  A hosted broker is wss://; a local one is ws://.
  broker: "wss://broker.example/v1/tunnel",

  // The instance of this end, for the grant.
  instance: "extension",
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

const ends = SURFACES.map((surface) => [
  surface,
  createNearEnd(surface, { broker: CONFIG.broker, instance: CONFIG.instance }),
]);

const store = browserStore();

/* The user's broker, if they have pointed the extension at one. */
async function loadBroker() {
  if (!store) {
    return CONFIG.broker;
  }
  const held = await store.get("conduit.broker");
  return held && held["conduit.broker"] ? held["conduit.broker"] : CONFIG.broker;
}

/* The secret of one surface, if the user has approved one.
 *
 * The secret is the user's decision: it comes from the device flow,
 * and it is not the extension's to keep if the user takes it away. */
async function loadSecret(surfaceId) {
  if (!store) {
    return "";
  }
  const key = "conduit.secret." + surfaceId;
  const held = await store.get(key);
  return held && held[key] ? held[key] : "";
}

/* Keep the secret of one surface. */
async function saveSecret(surfaceId, secret) {
  if (!store) {
    return;
  }
  await store.set({ ["conduit.secret." + surfaceId]: secret });
}

/* Ask the broker for a code, and open the page where the user
 * approves it.
 *
 * The broker does not hand out a secret because an extension asked:
 * the user says yes first, in the broker's own page. */
async function askForApproval(surface) {
  const api = typeof browser !== "undefined" ? browser : chrome;
  const answer = await fetch(CONFIG.broker.replace(/^ws/, "http") + "/v1/device/code", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      surface: surface.id,
      scopes: surface.scopes,
    }),
  });
  if (!answer.ok) {
    return null;
  }
  const code = await answer.json();
  const page = CONFIG.broker.replace(/^ws/, "http") + code.url;
  if (api && api.tabs && api.tabs.create) {
    await api.tabs.create({ url: page });
  }
  return code.code;
}

/* Ask the broker whether the user has approved the code. */
async function pollForTheSecret(surface, code) {
  const answer = await fetch(CONFIG.broker.replace(/^ws/, "http") + "/v1/device/secret", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ code, instance: CONFIG.instance }),
  });
  if (!answer.ok) {
    return "";
  }
  return answer.text();
}

/* The message that arrives from the page or from the native side. */
async function onMessage(message) {
  if (!message || !message.action) {
    return;
  }
  switch (message.action) {
    case "publish": {
      /* Every surface publishes what it reads. */
      for (const [, end] of ends) {
        await end.publishState();
      }
      break;
    }
    case "status": {
      const statuses = ends.map(([, end]) => end.status());
      return {
        ok: true,
        surfaces: statuses,
        sequence: statuses.reduce((total, status) => total + status.sequence, 0),
        connected: statuses.every((status) => status.connected),
        agreed: statuses.every((status) => status.agreed),
      };
    }
    case "toggle": {
      const running = ends.some(([, end]) => end.status().connected);
      if (running) {
        for (const [, end] of ends) {
          end.stop();
        }
      } else {
        await start();
      }
      break;
    }
    case "stop":
      for (const [, end] of ends) {
        end.stop();
      }
      break;
    default:
      break;
  }
  return { ok: true };
}

/* The start of the tunnels: with the secrets the user gave, and a
 * device flow for each surface that has none. */
async function start() {
  CONFIG.broker = await loadBroker();
  for (const [surface, end] of ends) {
    end.setBroker(CONFIG.broker);
    const secret = await loadSecret(surface.id);
    if (secret) {
      end.setSecret(secret);
      await end.start();
      continue;
    }
    // There is no secret for this surface yet: ask for one, and wait
    // for the user.
    const code = await askForApproval(surface).catch(() => null);
    if (!code) {
      continue;
    }
    const timer = setInterval(async () => {
      const granted = await pollForTheSecret(surface, code).catch(() => "");
      if (granted) {
        clearInterval(timer);
        await saveSecret(surface.id, granted);
        end.setSecret(granted);
        await end.start();
      }
    }, 5000);
  }
}

/* Wire the messages, in whichever browser this is. */
const api = typeof browser !== "undefined" ? browser : chrome;
if (api && api.runtime && api.runtime.onMessage) {
  api.runtime.onMessage.addListener(onMessage);
  if (api.runtime.onInstalled) {
    api.runtime.onInstalled.addListener(start);
  }
  if (api.runtime.onStartup) {
    api.runtime.onStartup.addListener(start);
  }
}

/* In a service worker, the worker may be started by a message rather
 * than by an event, so start here as well. */
start();

export { onMessage, start };
