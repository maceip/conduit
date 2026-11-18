/*
 * The tests of the browser tab surface.
 *
 * The surface is tested against a mock of the browser's tabs API, so
 * the logic is checked here; the behaviour inside a real browser is
 * not, and the README says so.
 *
 * Run it with: node --test clients/shared/surface-browser.test.js
 */

import assert from "node:assert/strict";
import test from "node:test";
import { readTab, readTabs, SCHEMA, SCOPE } from "./surface-browser.js";

/* A browser whose tabs API says what the test says it says. */
function mockBrowser(tabs, fail = false) {
  return {
    tabs: {
      async query(query) {
        if (fail) {
          throw new Error("the tabs API is not there");
        }
        if (query.active) {
          return tabs.filter((tab) => tab.active);
        }
        return tabs;
      },
    },
  };
}

const TABS = [
  { url: "https://example.org/one", title: "One", windowId: 7, active: true },
  { url: "https://example.org/two", title: "Two", windowId: 7, active: false },
];

test("the surface names its schema and its scope", () => {
  assert.equal(SCHEMA, "dev.conduit.browser.tab/v1");
  assert.equal(SCOPE, "browser.tabs.read");
});

test("the surface reads the tab in front of the user", async () => {
  const tab = await readTab(mockBrowser(TABS));
  assert.deepEqual(tab, {
    url: "https://example.org/one",
    title: "One",
    window: 7,
    active: true,
  });
});

test("the surface reads every tab", async () => {
  const tabs = await readTabs(mockBrowser(TABS));
  assert.equal(tabs.length, 2);
  assert.equal(tabs[1].active, false);
});

test("the surface says nothing when the browser has no tabs API", async () => {
  assert.equal(await readTab(null), null);
  assert.deepEqual(await readTabs(null), []);
});

test("the surface says nothing when the browser refuses", async () => {
  assert.equal(await readTab(mockBrowser([], true)), null);
  assert.deepEqual(await readTabs(mockBrowser([], true)), []);
});

test("a tab without a url is a tab without a url", async () => {
  const tab = await readTab(mockBrowser([{ active: true, windowId: 3 }]));
  assert.equal(tab.url, null);
  assert.equal(tab.title, null);
  assert.equal(tab.window, 3);
});
