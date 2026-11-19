# catalogue: apps that ship an extension surface

which apps let a user install something into them, and what the surface is. first written 2026-11-18.

the companion doc, [`extension-surfaces.md`](extension-surfaces.md), sorts these surfaces by *kind* and says why kind matters. this one is the catalogue: 1 row per app; surface, what the third party gets, whether the user can see it.

last column:

* **listed** => the app shows the extension to the user by name, usually with a toggle.
* **hidden** => the surface exists and is user-installable, but the app doesn't surface it; the user finds it in a menu, a settings pane, or not at all.
* **secret** => no installer at all. the surface is an interface; knowing it exists is the only barrier.

rows marked **not checked** are known to exist but weren't verified against a source in this pass. they're listed so the gaps are visible, not so they can be cited.

---

## browsers

| app | surface | what the third party gets | visible? | source |
|---|---|---|---|---|
| **Chrome**, Edge, Brave, Opera, Vivaldi, Arc | Manifest V3 web extension | sandboxed JS bundle + permission list; service worker; host permissions | listed | [Chrome for Developers](https://developer.chrome.com/docs/extensions/develop/migrate/what-is-mv3) |
| **Firefox** | WebExtension (MV3) | same, with the `browser.*` namespace; manifest carries both `background.scripts` and `background.service_worker` | listed | [MDN](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json/background) |
| **Safari** | Safari web extension | JS + a **native app extension** that mediates; `sendNativeMessage` / `connectNative` to the containing app | listed | [Apple](https://developer.apple.com/documentation/safariservices/messaging-between-the-app-and-javascript-in-a-safari-web-extension) |
| **Chromium ≥ 148** | `browser.*` namespace in Chrome | the Firefox-compatible API, for every extension from Chrome 152 | listed | [MDN](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Build_a_cross-browser_extension) |

## apple apps: surfaces with no UI at all

| app | surface | what the third party gets | visible? | source |
|---|---|---|---|---|
| **Mail.app** | MailKit app extensions | 4 kinds: compose, action, content blocker, message security | **hidden**; per-extension toggle in Mail's settings, no extension list in the app's own UI | [MailKit](https://www.developer.apple.com/documentation/mailkit), [Build Mail app extensions](https://www.developer.apple.com/documentation/mailkit/build-mail-app-extensions), [WWDC21 10168](https://developer.apple.com/videos/play/wwdc2021/10168/), [Use Mail extensions in Mail on Mac](https://support.apple.com/guide/mail/use-mail-extensions-mlhla9a93cd5/mac) |
| **Finder** | Finder Sync extension | badges, labels, context menus for a synced folder | **hidden**; the extension is a facet in Finder's extension settings | [Finder Sync guide](https://developer.apple.com/library/archive/documentation/General/Conceptual/ExtensibilityPG/Finder.html) |
| **Xcode** | Xcode Source Editor extension | commands over a text buffer; Xcode draws the UI | **hidden** | [WWDC16 414](https://devstreaming-cdn.apple.com/videos/wwdc/2016/414k6h6qdzyihpowpaa/414/414_using_and_extending_the_xcode_source_editor.pdf) |
| **Safari** | Safari web extension + content blocker | the web extension above; content blockers are pure WebKit rules | listed / hidden | [Apple](https://developer.apple.com/documentation/technologyoverviews/app-extensions.md) |
| **macOS, every app** | app extension points: Spotlight import, notification service, file provider, network extension, audio unit, intents, share/action, quick look, widgets, custom keyboard, contact + call directory, iMessage, photo editing | usually **no UI at all**; they "simply provide information for the system to use" | **hidden**; macOS 15.4.1 carries **over 400** app extensions and plugins, most not listed in System Settings | [App Extensions overview](https://developer.apple.com/documentation/technologyoverviews/app-extensions.md), [eclectic light](https://eclecticlight.co/2025/04/23/an-overview-of-app-extensions-and-plugins-in-macos-sequoia/) |

## editors and IDEs

| app | surface | what the third party gets | visible? | source |
|---|---|---|---|---|
| **VS Code** | extension API; Visual Studio Marketplace | Node or worker extension in the shared extension host; API covers editor, workspace, language servers, debuggers | listed | [Extension Marketplace](https://code.visualstudio.com/docs/configure/extensions/extension-marketplace), [Extensibility overview](https://vscode-docs.readthedocs.io/en/latest/extensions/overview/) |
| **VS Code, enterprise** | private marketplace; allow list | same, hosted by the org | listed | [Enterprise extensions](https://code.visualstudio.com/docs/enterprise/extensions) |
| **Cursor** | Open VSX through a marketplace proxy | same extensions, with an allowed-extensions list and an install cooldown | listed | [Agentdex on Cursor](https://agentdex.app/blog/cursor-ai-extensions/) |
| **Antigravity** (Google) | ACP server (`agy acp`) that editors launch; its own extensions go *into* other IDEs | agent runs in the editor's chat panel | **hidden**; the standalone IDE's own extension surface isn't part of the documented surface | [Antigravity IDE extensions](https://antigravity.google/docs/ide/extensions) |
| **JetBrains** | plugin platform; JetBrains Marketplace | JVM plugin loaded by the IDE | listed | [JetBrains Marketplace docs](https://plugins.jetbrains.com/docs/) |
| **Zed** | Rust extensions, ACP support | in-process extension | listed | [Zed extensions](https://zed.dev/docs/extensions) |
| **Neovim** | Lua; plugin manager | in-process Lua, with LSP + treesitter | listed | [Neovim plugins](https://neovim.io/doc/user/plugins.html) |
| **Emacs** | ELisp | in-process, full editor access | listed | [Emacs Lisp](https://www.gnu.org/software/emacs/manual/html_node/elisp/Lisp-Introduction.html) |
| **Xcode, Sublime, Eclipse, Warp, Kiro, Junie, Tabnine, Augment** | various | various | various | not checked |

## writing and design apps

| app | surface | what the third party gets | visible? | source |
|---|---|---|---|---|
| **Obsidian** | community plugins: a `manifest.json` + `main.js` in the vault's plugin folder; unofficial registry | whole app API; a plugin can register its own HTTP routes | listed | [Obsidian plugin API](https://obsidian-typings.github.io/obsidian-typings/public/api/obsidian/internals/plugins/Plugins/), [Community plugins](https://help.obsidian.md/plugins) |
| **Obsidian** | local REST API plugin | Bearer-authenticated HTTP server on `127.0.0.1:27124` | **secret**; no installer, just an address + token | [REST API plugin](https://community.obsidian.md/plugins/rest-api) |
| **Figma** | Plugin API | read/write access to the file; **short-lived and always user-invoked**: "it's not possible to build plugins that run in the background" | listed, after approval | [Plugin API](https://developers.figma.com/docs/plugins/), [About building plugins](https://help.figma.com/hc/en-us/articles/41407987481879-About-building-plugins-in-Figma) |
| **Notion** | developer platform + integrations | OAuth integration the user grants a page to | listed | [Notion developers](https://developers.notion.com/) |
| **WordPress** | plugins and themes, per site | PHP plugin, in the site | listed | [Plugin handbook](https://developer.wordpress.org/plugins/) |

## agents: the surfaces that are growing

| app | surface | what the third party gets | visible? | source |
|---|---|---|---|---|
| **MCP** | Extensions framework: reverse-DNS ids (`io.modelcontextprotocol/ui`), negotiated through the `extensions` capability map, versioned independently | tools, resources, prompts, tasks; MCP Apps declare UI with the `ui://` scheme and can hide a tool from the model | listed | [MCP extensions](https://modelcontextprotocol.io/extensions/overview), [MCP Apps](https://github.com/modelcontextprotocol/ext-apps/blob/v2.0.3/specification/2026-01-26/apps.mdx), [release notes](https://blog.modelcontextprotocol.io/posts/2026-07-28-release-candidate/) |
| **ACP** | Agent Client Protocol | lets an editor launch and drive an agent | listed | [ACP](https://agentclientprotocol.com/) |
| **VS Code** | MCP servers from the marketplace | a server the agent calls | listed | [VS Code extensibility](https://code.visualstudio.com/features/extensibility) |
| **Antigravity** | `agy acp` | the agent, driven from another editor | hidden | [Antigravity](https://antigravity.google/docs/ide/extensions) |
| **Cursor, Claude Code, the Agent Plugins standard** | plugin directories with a marketplace | packaged agent: commands, agents, hooks, MCP servers | listed | not checked |

## secret surfaces: no installer at all

these are what the user meant by "it can just be like a secret API or anything". none has an extension *store*; each is an interface you have to know about.

| surface | what it is | how it's reached | source |
|---|---|---|---|
| **Obsidian's local REST API** | HTTP API over the vault | `127.0.0.1:27124`, Bearer token | [the plugin](https://community.obsidian.md/plugins/rest-api) |
| **AppleScript / Automation** | scriptable access to most apps | `osascript`, or an app that asks for automation permission | not checked |
| **Shortcuts** | library of user-made actions | Shortcuts app | not checked |
| **Home Assistant** | device integrations | YAML or a custom component, installed from the UI | not checked |
| **Docker socket API** | control of the container runtime | Unix socket | not checked |
| **Chrome DevTools Protocol** | full control of a browser | WebSocket on the debug port | not checked |
| **mitmproxy add-ons, Postman pre-request scripts, Wireshark Lua** | scripts inside a network tool | the tool's own loader | not checked |
| **D-Bus, MPD, Audio Units, VST3** | interfaces inside the desktop | system bus, protocol, plugin host | not checked |

## what the catalogue says

1. visible surfaces are all called "plugins"; invisible ones are all "extensions" or "integrations". the naming gives it away: an *extension* is something the OS registers and the user usually never sees; a *plugin* is something the app shows you.
2. surfaces with no UI are the most useful and the least discoverable. a Finder Sync extension, a Spotlight importer, a local REST API: none needs a window => nobody built a UI for them, and a tunnel fits them.
3. 2 constraints from the catalogue shape the transport. Figma plugins are short-lived + user-invoked => a Figma surface can't hold a socket; Chrome's worker dies after 30s => no browser surface can hold one either. so the socket lives outside the app, and the app pushes into it when it's awake.
4. the growing surfaces are agent surfaces: MCP's extensions framework, ACP, agent plugin directories. the client side of this project is an agent far more often than a human.
5. nothing in the catalogue ships a tunnel. every entry is in-process (a plugin), a local socket (a secret API), or a document protocol (MCP). this project fills the gap between "in-process" and "over the internet", with a user in the middle who can see and revoke it.
