// The Figma plugin's main thread.
//
// Figma runs this in its own sandbox, with the `figma` API object.  It
// cannot open a socket, so it does not try: it reads the selection and
// posts it to the UI iframe, which holds the connection to the near
// end.

/** The iframe that holds the near end connection. */
function showUi() {
  if (typeof figma === "undefined") {
    return;
  }
  figma.showUI(__html__, { width: 320, height: 160 });
}

/** Tell the iframe what the user has selected. */
function publishSelection() {
  if (typeof figma === "undefined") {
    return;
  }
  const selection =
    figma.currentPage && figma.currentPage.selection
      ? figma.currentPage.selection.map((node) => ({
          id: node.id || "",
          name: node.name || "",
          type: node.type || "",
        }))
      : [];
  figma.ui.postMessage({ type: "selection", selection });
}

/** The plugin is invoked: show the iframe and say what is selected. */
function main() {
  showUi();
  publishSelection();
  figma.on("selectionchange", publishSelection);
}

main();
