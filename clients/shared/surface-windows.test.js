/*
 * The tests of the browser windows surface.
 *
 * The surface is tested against a mock of the browser's windows API —
 * the same kind of mock the tab surface uses — so the logic is checked
 * here and the near end is checked by the browser run.
 *
 * Run it with: node --test clients/shared/surface-windows.test.js
 */

import assert from "node:assert/strict";
import test from "node:test";
import { readWindow, readWindows, SCHEMA, CHANNEL, SCOPE } from "./surface-windows.js";

/* A browser whose windows API says what the test says it says. */
function mockBrowser(windows, fail = false) {
  return {
    windows: {
      async getAll() {
        if (fail) {
          throw new Error("the windows API is not there");
        }
        return windows;
      },
    },
  };
}

const WINDOWS = [
  { id: 3, focused: true, state: "normal", tabs: [{}, {}] },
  { id: 4, focused: false, state: "minimized", tabs: [{}] },
];

test("the surface names its schema, its channel and its scope", () => {
  assert.equal(SCHEMA, "dev.conduit.browser.window/v1");
  assert.equal(CHANNEL, "windows");
  assert.equal(SCOPE, "browser.windows.read");
});

test("the surface reads every window", async () => {
  const windows = await readWindows(mockBrowser(WINDOWS));
  assert.deepEqual(windows, [
    { id: 3, focused: true, state: "normal", tabs: 2 },
    { id: 4, focused: false, state: "minimized", tabs: 1 },
  ]);
});

test("the surface reads the window the user is looking at", async () => {
  const window = await readWindow(mockBrowser(WINDOWS));
  assert.deepEqual(window, { id: 3, focused: true, state: "normal", tabs: 2 });
});

test("the surface says nothing when the browser has no windows API", async () => {
  assert.deepEqual(await readWindows(null), []);
  assert.equal(await readWindow(null), null);
});

test("the surface says nothing when the browser refuses", async () => {
  assert.deepEqual(await readWindows(mockBrowser([], true)), []);
  assert.equal(await readWindow(mockBrowser([], true)), null);
});

test("a window without an id is a window without an id", async () => {
  const windows = await readWindows(mockBrowser([{ focused: true, tabs: [] }]));
  assert.deepEqual(windows, [{ id: null, focused: true, state: null, tabs: 0 }]);
});

test("when no window is focused the first one stands in", async () => {
  const window = await readWindow(
    mockBrowser([
      { id: 9, focused: false, state: "normal", tabs: [] },
      { id: 10, focused: false, state: "normal", tabs: [] },
    ]),
  );
  assert.equal(window.id, 9);
});
