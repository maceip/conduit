// The VS Code extension.
//
// VS Code runs an extension in its own extension host process, and
// that process dies with the window.  So the extension does not hold a
// socket to the broker: it publishes to the near end, which does.
//
// The extension is what VS Code loads; `publish.js` is what it uses.

const path = require("node:path");
const vscode = require("vscode");
const { connect } = require("../../clients/native/near-end.js");
const { publishTab, TAB_SCHEMA } = require("./publish.js");

/** Where the near end is, if the user has said. */
function socketPath() {
  const setting = vscode.workspace
    .getConfiguration("conduit")
    .get("nearEndSocket");
  return setting || process.env.CONDUIT_SOCKET || "";
}

/** The tab VS Code has in front of the user. */
function activeTab() {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    return null;
  }
  const document = editor.document;
  return {
    title: path.basename(document.fileName),
    language: document.languageId,
    path: document.uri.fsPath,
    url: document.uri.toString(),
    dirty: document.isDirty,
  };
}

async function activate(context) {
  const socket = socketPath();
  if (!socket) {
    // Nothing to publish to until the user says where the near end is.
    return;
  }
  const client = connect(socket);

  context.subscriptions.push(
    vscode.commands.registerCommand("conduit.publishActiveTab", async () => {
      const tab = activeTab();
      if (tab) {
        await publishTab(client, tab);
      }
    }),
  );

  // Publish the tab in front of the user whenever it changes.
  context.subscriptions.push(
    vscode.window.onDidChangeActiveTextEditor(async (editor) => {
      if (!editor) {
        return;
      }
      const document = editor.document;
      await publishTab(client, {
        title: path.basename(document.fileName),
        language: document.languageId,
        path: document.uri.fsPath,
        url: document.uri.toString(),
        dirty: document.isDirty,
      });
    }),
  );

  // And once, when the extension starts.
  const tab = activeTab();
  if (tab) {
    await publishTab(client, tab);
  }
}

function deactivate() {}

module.exports = { activate, deactivate, TAB_SCHEMA };
