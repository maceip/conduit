/*
 * The browser tab surface.
 *
 * The first surface of the project: what the browser knows about the
 * tab in front of the user.  It is deliberately small — the point of
 * the exercise is the tunnel, not the payload — but it is a real
 * surface, with a schema, a scope and a channel.
 *
 * The same file serves Chromium and Safari: the two browsers have the
 * same tabs API under different names, and `browser` is the standard
 * one, which Safari and Firefox both have, while Chromium has it from
 * Chrome 148 and `chrome` before that.
 */

export const SCHEMA = "dev.conduit.browser.tab/v1";
export const CHANNEL = "tabs";
export const SCOPE = "browser.tabs.read";

/* The browser's API, whichever it is. */
export function api() {
  if (typeof browser !== "undefined" && browser.tabs) {
    return browser;
  }
  if (typeof chrome !== "undefined" && chrome.tabs) {
    return chrome;
  }
  return null;
}

/* The tab in front of the user.
 *
 * The URL is the one the browser shows, which may be a page inside a
 * site the user is allowed to read and not the site's own address, so
 * the surface publishes what the browser says rather than what it
 * guesses. */
export async function readTab(browser = api()) {
  if (!browser) {
    return null;
  }
  let tabs;
  try {
    tabs = await browser.tabs.query({ active: true, lastFocusedWindow: true });
  } catch (error) {
    return null;
  }
  const tab = tabs && tabs[0];
  if (!tab) {
    return null;
  }
  return {
    url: tab.url || null,
    title: tab.title || null,
    window: typeof tab.windowId === "number" ? tab.windowId : null,
    active: true,
  };
}

/* The tabs of every window, for the surfaces that want more than one. */
export async function readTabs(browser = api()) {
  if (!browser) {
    return [];
  }
  try {
    const tabs = await browser.tabs.query({});
    return (tabs || []).map((tab) => ({
      url: tab.url || null,
      title: tab.title || null,
      window: typeof tab.windowId === "number" ? tab.windowId : null,
      active: !!tab.active,
    }));
  } catch (error) {
    return [];
  }
}
