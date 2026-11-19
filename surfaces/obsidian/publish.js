// What the Obsidian surface publishes.
//
// The plugin has two halves: this one, which knows what a note is and
// what to say about it, and `main.js`, which is the half Obsidian
// loads.  The split is deliberate: this half has no dependency on
// Obsidian, so the check can drive the very same code that the vault
// drives.

/** The schema of a note. */
const NOTE_SCHEMA = "dev.conduit.obsidian.note/v1";

/**
 * What is said about a note.
 *
 * @param {object} note the note: a path, a basename, some text
 * @returns {object} the payload
 */
function notePayload(note) {
  return {
    path: note.path || "",
    basename: note.basename || "",
    heading: note.heading || "",
    tags: Array.isArray(note.tags) ? note.tags : [],
    excerpt: note.excerpt || "",
  };
}

/**
 * Publish a note.
 *
 * @param {object} client the client of the near end
 * @param {object} note the note
 * @returns {Promise<void>}
 */
async function publishNote(client, note) {
  await client.publish(NOTE_SCHEMA, notePayload(note));
}

/**
 * The note that is in front of the user, as Obsidian would say it.
 *
 * @param {object} app the Obsidian API object
 * @returns {object|null} the note, or null when there is none
 */
function noteFromApp(app) {
  const file = app && app.workspace ? app.workspace.getActiveFile() : null;
  if (!file) {
    return null;
  }
  const note = {
    path: file.path || "",
    basename: file.basename || "",
    excerpt: "",
  };
  const view = app.workspace.getActiveViewOfType
    ? app.workspace.getActiveViewOfType
    : null;
  void view;
  if (app.metadataCache && app.metadataCache.getFileCache) {
    const cache = app.metadataCache.getFileCache(file);
    if (cache && cache.headings && cache.headings.length) {
      note.heading = cache.headings[cache.headings.length - 1].heading || "";
    }
  }
  if (app.metadataCache && app.metadataCache.getFileCache) {
    const cache = app.metadataCache.getFileCache(file);
    if (cache && cache.tags && cache.tags.length) {
      note.tags = cache.tags.map((tag) => tag.tag || "").filter(Boolean);
    }
  }
  return note;
}

module.exports = { NOTE_SCHEMA, notePayload, publishNote, noteFromApp };
