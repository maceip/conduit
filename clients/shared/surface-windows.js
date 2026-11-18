/*
 * The browser windows surface.
 *
 * The second surface of the project, and the proof that a surface is a
 * module: an identifier, a channel, a scope, and a read.  The near end
 * carries it the same way it carries the tab surface — the tunnel
 * machinery knows nothing about either.
 *
 * What it reads is what the browser's windows API says: the windows,
 * whether one is focused, and how many tabs each holds.  It is the same
 * API on Chromium and Safari, under the same `browser` / `chrome` names
 * as the tabs API.
 */

export const SCHEMA = "dev.conduit.browser.window/v1";
export const CHANNEL = "windows";
export const SCOPE = "browser.windows.read";

/* The browser's API, whichever it is. */
export function api() {
  if (typeof browser !== "undefined" && browser.windows) {
    return browser;
  }
  if (typeof chrome !== "undefined" && chrome.windows) {
    return chrome;
  }
  return null;
}

/* The windows the browser has open.
 *
 * The count of tabs is a number, not the tabs themselves: a window is
 * the subject of this surface, and the tab surface is where a tab is
 * read. */
export async function readWindows(browser = api()) {
  if (!browser) {
    return [];
  }
  let windows;
  try {
    windows = await browser.windows.getAll({ populate: true });
  } catch (error) {
    return [];
  }
  return (windows || []).map((window) => ({
    id: typeof window.id === "number" ? window.id : null,
    focused: !!window.focused,
    state: typeof window.state === "string" ? window.state : null,
    tabs: Array.isArray(window.tabs) ? window.tabs.length : 0,
  }));
}

/* The window the user is looking at. */
export async function readWindow(browser = api()) {
  if (!browser) {
    return null;
  }
  let window;
  try {
    const windows = await browser.windows.getAll({ populate: true });
    window = (windows || []).find((one) => one.focused) || (windows && windows[0]);
  } catch (error) {
    return null;
  }
  if (!window) {
    return null;
  }
  return {
    id: typeof window.id === "number" ? window.id : null,
    focused: !!window.focused,
    state: typeof window.state === "string" ? window.state : null,
    tabs: Array.isArray(window.tabs) ? window.tabs.length : 0,
  };
}
