// What the Safari surface publishes.
//
// The browser extension is the half that runs in Safari; this is the
// half that knows what a tab is.  It is the same module the Chromium
// extension's surface uses in spirit and the same one the check drives.

/** The schema of a tab. */
export const TAB_SCHEMA = "dev.conduit.browser.tab/v1";

/**
 * What is said about a tab.
 *
 * @param {object} tab the tab: a title and a url
 * @returns {object} the payload
 */
export function tabPayload(tab) {
  return {
    title: tab.title || "",
    url: tab.url || "",
    active: Boolean(tab.active),
  };
}

/**
 * Publish a tab.
 *
 * @param {object} client the client of the near end
 * @param {object} tab the tab
 * @returns {Promise<void>}
 */
export async function publishTab(client, tab) {
  await client.publish(TAB_SCHEMA, tabPayload(tab));
}
