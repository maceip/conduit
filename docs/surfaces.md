# The surfaces

A **surface** is a place a Conduit client can talk to.  Every surface
has the same three parts, whatever the app is: the app, the plugin
inside it, and the **near end** — the process that holds the socket to
the broker and does the sealing.

```
   the app  ──►  the plugin  ──►  the near end  ──►  the broker  ──►  the client
                (plain JSON)      (seals it)       (forwards it)   (opens it)
```

The plugin never sees a key and never holds a socket.  That is not a
style choice: a VS Code extension host dies with the window, a Safari
background page is unloaded after thirty seconds, a Mail app extension
is torn down the moment the message is sent, and a Slack connection
drops.  None of them can be trusted with a socket, and none of them
should be trusted with a key.

The contract every surface follows is in
[`surface-contract.md`](surface-contract.md).  The near end is in
[`crates/conduit-near-end`](../crates/conduit-near-end).

## What is here

| the surface | the app | the plugin is | what it publishes |
|---|---|---|---|
| [`apple-mail`](../surfaces/apple-mail) | Apple Mail | a MailKit app extension | the message being composed, the message selected |
| [`apple-shortcuts`](../surfaces/apple-shortcuts) | Apple Shortcuts | an App Intent | what the owning app says about the state |
| [`antigravity`](../surfaces/antigravity) | Google Antigravity | a plugin: `plugin.json`, a skill, an MCP server | the state of the surface, as an MCP tool |
| [`discord`](../surfaces/discord) | Discord | a bot | the message of the channel |
| [`figma`](../surfaces/figma) | Figma | a plugin: `manifest.json`, code and UI | the current selection |
| [`home-assistant`](../surfaces/home-assistant) | Home Assistant | a custom integration | the state of the entities the user listed |
| [`mcp`](../surfaces/mcp) | any MCP host | an MCP server over stdio | the state of the surface, as an MCP tool |
| [`neovim`](../surfaces/neovim) | Neovim | a Lua plugin | the buffer in front of the user |
| [`obsidian`](../surfaces/obsidian) | Obsidian | a community plugin | the note in front of the user |
| [`safari`](../surfaces/safari) | Safari | the web extension and the native host | the tab in front of the user |
| [`slack`](../surfaces/slack) | Slack | a Socket Mode app | the message of the channel |
| [`stream-deck`](../surfaces/stream-deck) | Elgato Stream Deck | an action plugin | the key that was pressed |
| [`thunderbird`](../surfaces/thunderbird) | Thunderbird | a WebExtension | the message in front of the user |
| [`vscode`](../surfaces/vscode) | VS Code | an extension | the tab in front of the user |
| [`wordpress`](../surfaces/wordpress) | WordPress | a plugin | the site, and each post |

Plus the two surfaces of the browser extension itself, in
`clients/shared/`: the tab and the windows.

## What was verified, and what was not

This is the honest ledger.  A surface that was never run is not a
surface that works, and the word **unverified** is used where it
applies.

| the surface | verified here | not verified here |
|---|---|---|
| apple-mail | the `Info.plist` parses and the declaration is right | **not run**: no macOS, no Xcode, no Swift. The extension was never compiled, signed, registered or loaded by Mail. |
| apple-shortcuts | nothing | **not run**: no Swift toolchain. The App Intent was never compiled and never appeared in Shortcuts. |
| antigravity | the plugin's files and the MCP server it ships, driven over stdio | **not run**: Antigravity itself. The IDE was never launched, the plugin was never installed or validated by `agy`, and the skill was never loaded by an agent. |
| discord | the publishing half, end to end, against a real broker and a real near end | the Discord side: no application, no token, no guild. The Gateway handshake, the heartbeat and the slash command never ran. |
| figma | the publishing half, over the loopback | Figma itself: the plugin was never installed or run in the editor. |
| home-assistant | the publishing half (`publish.py`, the module `__init__.py` imports), end to end | Home Assistant itself: the integration was never loaded by HA, and the event bus, `hass.states.get` and the config entry never ran. |
| mcp | the MCP server, end to end, over stdio, against a real broker and a real near end | any real MCP host or SDK: the check speaks JSON-RPC by hand. The SEP-2133 extensions framework is not implemented. |
| neovim | nothing | **not run**: no Neovim, no Lua. The plugin was never loaded. |
| obsidian | the publishing half, and the plugin class driven with a fake app object | Obsidian itself: the plugin was never loaded by Obsidian. |
| safari | the near end in its native mode, spoken to over a pipe exactly as Safari speaks to it | Safari itself: the extension was never loaded by Safari, and the native messaging host was never registered with it. |
| slack | the publishing half, end to end | the Slack side: no app, no token. The Socket Mode handshake, the envelope ack and the slash command never ran. |
| stream-deck | the publishing half, and the action's shape accepted by the real SDK | the device: the plugin never ran on a Stream Deck. |
| thunderbird | the publishing half, over the loopback | Thunderbird itself: the extension was never loaded by Thunderbird. |
| vscode | the publishing half — the module `extension.js` imports — end to end | VS Code itself: the extension was never loaded by an editor, because the VS Code API is not on this machine. |
| wordpress | nothing | **not run**: PHP is not installed. Neither file was ever parsed and no hook ever fired. |

## What a check is

A check starts a **real broker**, starts a **real near end** with the
surface's own declaration, runs the **plugin's own code** — not a copy
of it — and sees the client at the far end receive the payload, opened.
Nothing in a check is mocked.

```sh
cargo build -p conduit-broker -p conduit-near-end
node scripts/surfaces/run.mjs              # every surface
node scripts/surfaces/run.mjs vscode mcp   # some of them
```

A surface with no check is reported as *not run*, not as passed.

## The two surfaces the browser extension has

The browser extension carries two surfaces of its own, in
`clients/shared/`: `surface-browser.js` (the tab in front of the user)
and `surface-windows.js` (the windows the browser has open).  Those are
verified in a real Chromium, by `scripts/browser-test.mjs` and by
`scripts/e2e/run.sh`.

## The surfaces that do not exist

**WhatsApp has no extension surface.**  There is nothing a user can
install into WhatsApp.  Everything third-party is server-side: the
Business Platform's Cloud API and webhooks, and the Flow JSON a
business publishes — and a Flow is published by the business, not
installed by the user.  WhatsApp Web is a web page, so a browser
extension can inject into it, but such an extension is unofficial and
WhatsApp's terms forbid reverse engineering and third-party tools that
function as the service.  So: no WhatsApp surface here, and the reason
is in [`research/app-catalogue.md`](research/app-catalogue.md).

**Antigravity's own extension surface is its plugin system**, not VS
Code extensions: `plugin.json`, skills, rules, hooks and MCP servers,
loaded from `.agents/plugins/` or `~/.gemini/config/plugins/`.  Whether
the standalone IDE also loads stock VS Code `.vsix` extensions is **not
documented by Google**, so the Antigravity plugin here is built on the
documented plugin system only.  Sources:
[antigravity.google/docs/plugins](https://antigravity.google/docs/plugins),
[antigravity.google/docs/build-plugins](https://antigravity.google/docs/build-plugins).
