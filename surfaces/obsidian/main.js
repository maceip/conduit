// The Obsidian plugin.
//
// Obsidian loads a CommonJS module whose default export is a class
// with an `onload`, and it hands the class the Obsidian API object.
// The plugin holds no key and opens no socket to the broker: it
// publishes to the near end, which does the sealing.
//
// The publishing half is in `publish.js`, which knows nothing about
// Obsidian.

const path = require("node:path");
const { connect } = require("../../clients/native/near-end.js");
const { publishNote, noteFromApp } = require("./publish.js");

/** Where the near end is, if the user has said. */
function socketPath() {
  return process.env.CONDUIT_SOCKET || "";
}

module.exports = class ConduitPlugin extends require("obsidian").Plugin {
  async onload() {
    const socket = socketPath();
    if (!socket) {
      // Nothing to publish to until the user says where the near end
      // is.  The setting is read when there is one.
      this.addSettingTab(new ConduitSettingTab(this.app, this));
      return;
    }
    const client = connect(socket);

    // Publish the note in front of the user whenever it changes.
    this.registerEvent(
      this.app.workspace.on("file-open", async (file) => {
        if (!file) {
          return;
        }
        await publishNote(client, {
          path: file.path || "",
          basename: file.basename || path.basename(file.path || ""),
          excerpt: "",
        });
      }),
    );

    // And once, when the plugin loads.
    const note = noteFromApp(this.app);
    if (note) {
      await publishNote(client, note);
    }
  }

  onunload() {}
};

/** The settings: where the near end is. */
class ConduitSettingTab extends require("obsidian").PluginSettingTab {
  constructor(app, plugin) {
    super(app, plugin);
    this.plugin = plugin;
  }

  display() {
    const { Setting } = require("obsidian");
    new Setting(this.containerEl)
      .setName("Near end socket")
      .setDesc("The Unix socket of the Conduit near end.")
      .addText((text) =>
        text
          .setPlaceholder("/tmp/conduit-obsidian.sock")
          .setValue(this.plugin.settings ? this.plugin.settings.socket : "")
          .onChange(async (value) => {
            this.plugin.settings = Object.assign({}, this.plugin.settings, {
              socket: value,
            });
            await this.plugin.saveData(this.plugin.settings);
          }),
      );
  }
}
