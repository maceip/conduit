// What the VS Code surface publishes.
//
// The plugin has two halves: this one, which knows what a tab is and
// what to say about it, and `extension.js`, which is the part VS Code
// loads.  The split is deliberate: this half has no dependency on
// VS Code, so the check can drive the very same code that the editor
// drives.

/** The schema of a tab. */
const TAB_SCHEMA = "dev.conduit.vscode.tab/v1";

/**
 * What is said about a tab.
 *
 * @param {object} tab the tab: a title, a document, a language
 * @returns {object} the payload
 */
function tabPayload(tab) {
  return {
    title: tab.title || "",
    language: tab.language || "",
    path: tab.path || "",
    url: tab.url || "",
    dirty: Boolean(tab.dirty),
  };
}

/**
 * Publish a tab.
 *
 * @param {object} client the client of the near end
 * @param {object} tab the tab
 * @returns {Promise<void>}
 */
async function publishTab(client, tab) {
  await client.publish(TAB_SCHEMA, tabPayload(tab));
}

module.exports = { TAB_SCHEMA, tabPayload, publishTab };
