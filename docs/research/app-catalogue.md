# The catalogue: apps that ship an extension surface

**Which apps let a user install something into them, and what the surface
is.**  First written 2026-11-18.

The companion document,
[`extension-surfaces.md`](extension-surfaces.md), sorts these surfaces by
*kind* and says why the kind matters.  This one is the catalogue: one
row per app, with the surface, what the third party gets, and whether the
user can see it.

**How to read the last column.**

* **listed** — the app shows the extension to the user, with a name, and
  usually a toggle.
* **hidden** — the surface exists and is user-installable, but the app
  does not surface it; the user finds it in a menu, a settings pane, or
  not at all.
* **secret** — there is no installer at all.  The surface is an
  interface, and knowing about it is the whole barrier.

Rows marked **not checked** are known to exist but were not verified
against a source in this pass.  They are listed so that the gaps are
visible, not so that they can be cited.

---

## The browsers

| app | the surface | what the third party gets | visible? | source |
|---|---|---|---|---|
| **Chrome**, Edge, Brave, Opera, Vivaldi, Arc | Manifest V3 web extension | a sandboxed JS bundle with a permission list; a service worker; host permissions | listed | [Chrome for Developers](https://developer.chrome.com/docs/extensions/develop/migrate/what-is-mv3) |
| **Firefox** | WebExtension (MV3) | the same, with the `browser.*` namespace; the manifest carries both `background.scripts` and `background.service_worker` | listed | [MDN](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json/background) |
| **Safari** | Safari web extension | JS plus a **native app extension** that mediates; `sendNativeMessage` / `connectNative` to the containing app | listed | [Apple](https://developer.apple.com/documentation/safariservices/messaging-between-the-app-and-javascript-in-a-safari-web-extension) |
| **Chromium ≥ 148** | `browser.*` namespace in Chrome | the Firefox-compatible API, for every extension from Chrome 152 | listed | [MDN](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Build_a_cross-browser_extension) |

## The Apple apps — the surface with no UI at all

| app | the surface | what the third party gets | visible? | source |
|---|---|---|---|---|
| **Mail.app** | MailKit app extensions | four kinds: compose, action, content blocker, message security | **hidden** — a per-extension toggle in Mail's settings, no list of extensions in the app's own UI | [MailKit](https://www.developer.apple.com/documentation/mailkit), [Build Mail app extensions](https://www.developer.apple.com/documentation/mailkit/build-mail-app-extensions), [WWDC21 10168](https://developer.apple.com/videos/play/wwdc2021/10168/), [Use Mail extensions in Mail on Mac](https://support.apple.com/guide/mail/use-mail-extensions-mlhla9a93cd5/mac) |
| **Finder** | Finder Sync extension | badges, labels, context menus for a synced folder | **hidden** — the extension is a facet in Finder's extension settings | [Finder Sync guide](https://developer.apple.com/library/archive/documentation/General/Conceptual/ExtensibilityPG/Finder.html) |
| **Xcode** | Xcode Source Editor extension | commands over a text buffer; Xcode draws the UI | **hidden** | [WWDC16 414](https://devstreaming-cdn.apple.com/videos/wwdc/2016/414k6h6qdzyihpowpaa/414/414_using_and_extending_the_xcode_source_editor.pdf) |
| **Safari** | Safari web extension + content blocker | the web extension above; content blockers are pure WebKit rules | listed / hidden | [Apple](https://developer.apple.com/documentation/technologyoverviews/app-extensions.md) |
| **macOS, every app** | app extension points: Spotlight import, notification service, file provider, network extension, audio unit, intents, share/action, quick look, widgets, custom keyboard, contact and call directory, iMessage, photo editing | usually **no UI at all** — "simply provide information for the system to use" | **hidden** — macOS 15.4.1 carries **over 400** app extensions and plugins, most of them not listed in System Settings | [App Extensions overview](https://developer.apple.com/documentation/technologyoverviews/app-extensions.md), [eclectic light](https://eclecticlight.co/2025/04/23/an-overview-of-app-extensions-and-plugins-in-macos-sequoia/) |

## The editors and IDEs

| app | the surface | what the third party gets | visible? | source |
|---|---|---|---|---|
| **VS Code** | extension API; the Visual Studio Marketplace | a Node or worker extension in the shared extension host; the API covers the editor, the workspace, the language servers, the debuggers | listed | [Extension Marketplace](https://code.visualstudio.com/docs/configure/extensions/extension-marketplace), [Extensibility overview](https://vscode-docs.readthedocs.io/en/latest/extensions/overview/) |
| **VS Code, enterprise** | a private marketplace; an allow list | the same, hosted by the organisation | listed | [Enterprise extensions](https://code.visualstudio.com/docs/enterprise/extensions) |
| **Cursor** | Open VSX through a marketplace proxy | the same extensions, with an allowed-extensions list and an install cooldown | listed | [Agentdex on Cursor](https://agentdex.app/blog/cursor-ai-extensions/) |
| **Antigravity** (Google) | an ACP server (`agy acp`) that editors launch; its own extensions go *into* other IDEs | the agent runs in the editor's chat panel | **hidden** — the standalone IDE's own extension surface is not part of the documented surface | [Antigravity IDE extensions](https://antigravity.google/docs/ide/extensions) |
| **JetBrains** | the plugin platform; the JetBrains Marketplace | a JVM plugin loaded by the IDE | listed | [JetBrains Marketplace docs](https://plugins.jetbrains.com/docs/) |
| **Zed** | Rust extensions, with ACP support | an in-process extension | listed | [Zed extensions](https://zed.dev/docs/extensions) |
| **Neovim** | Lua; the plugin manager | in-process Lua, with LSP and treesitter | listed | [Neovim plugins](https://neovim.io/doc/user/plugins.html) |
| **Emacs** | ELisp | in-process, with the full editor | listed | [Emacs Lisp](https://www.gnu.org/software/emacs/manual/html_node/elisp/Lisp-Introduction.html) |
| **Xcode, Sublime, Eclipse, Warp, Kiro, Junie, Tabnine, Augment** | various | various | various | not checked |

## The writing and design apps

| app | the surface | what the third party gets | visible? | source |
|---|---|---|---|---|
| **Obsidian** | community plugins: a `manifest.json` and a `main.js` in the vault's plugin folder, with an unofficial registry | the whole app API; a plugin may register its own HTTP routes | listed | [Obsidian plugin API](https://obsidian-typings.github.io/obsidian-typings/public/api/obsidian/internals/plugins/Plugins/), [Community plugins](https://help.obsidian.md/plugins) |
| **Obsidian** | the local REST API plugin | a Bearer-authenticated HTTP server on `127.0.0.1:27124` | **secret** — no installer, an address and a token | [REST API plugin](https://community.obsidian.md/plugins/rest-api) |
| **Figma** | the Plugin API | read and write access to the file; **short-lived and always user-invoked** — "it's not possible to build plugins that run in the background" | listed, after approval | [Plugin API](https://developers.figma.com/docs/plugins/), [About building plugins](https://help.figma.com/hc/en-us/articles/41407987481879-About-building-plugins-in-Figma) |
| **Notion** | the developer platform and integrations | an OAuth integration the user grants a page to | listed | [Notion developers](https://developers.notion.com/) |
| **WordPress** | plugins and themes, per site | a PHP plugin, in the site | listed | [Plugin handbook](https://developer.wordpress.org/plugins/) |

## The agents — the surfaces that are growing

| app | the surface | what the third party gets | visible? | source |
|---|---|---|---|---|
| **MCP** | the Extensions framework: reverse-DNS ids (`io.modelcontextprotocol/ui`), negotiated through the `extensions` capability map, versioned independently | tools, resources, prompts, tasks; MCP Apps declare UI by the `ui://` scheme and can hide a tool from the model | listed | [MCP extensions](https://modelcontextprotocol.io/extensions/overview), [MCP Apps](https://github.com/modelcontextprotocol/ext-apps/blob/v2.0.3/specification/2026-01-26/apps.mdx), [release notes](https://blog.modelcontextprotocol.io/posts/2026-07-28-release-candidate/) |
| **ACP** | the Agent Client Protocol | a way for an editor to launch and drive an agent | listed | [ACP](https://agentclientprotocol.com/) |
| **VS Code** | MCP servers from the marketplace | a server the agent calls | listed | [VS Code extensibility](https://code.visualstudio.com/features/extensibility) |
| **Antigravity** | `agy acp` | the agent, driven from another editor | hidden | [Antigravity](https://antigravity.google/docs/ide/extensions) |
| **Cursor, Claude Code, the Agent Plugins standard** | plugin directories with a marketplace | a packaged agent, with commands, agents, hooks and MCP servers | listed | not checked |

## The secret surfaces — no installer at all

These are the ones the user was reaching for with "it can just be like a
secret API or anything".  None of them has an extension *store*; each is
an interface you have to know about.

| surface | what it is | how it is reached | source |
|---|---|---|---|
| **Obsidian's local REST API** | an HTTP API over the vault | `127.0.0.1:27124`, Bearer token | [the plugin](https://community.obsidian.md/plugins/rest-api) |
| **AppleScript / Automation** | scriptable access to most apps | `osascript`, or an app that asks for automation permission | not checked |
| **Shortcuts** | a library of user-made actions | the Shortcuts app | not checked |
| **Home Assistant** | integrations for devices | YAML or a custom component, installed from the UI | not checked |
| **The Docker socket API** | control of the container runtime | a Unix socket | not checked |
| **The Chrome DevTools Protocol** | control of a browser, fully | a WebSocket on the debug port | not checked |
| **mitmproxy add-ons, Postman pre-request scripts, Wireshark Lua** | scripts inside a network tool | the tool's own loader | not checked |
| **D-Bus, MPD, Audio Units, VST3** | interfaces inside the desktop | the system bus, the protocol, the plugin host | not checked |

## What the catalogue says

1. **The visible surfaces are all "plugins" and the invisible ones are
   all "extensions" or "integrations".**  The naming is the tell: an
   *extension* is something the OS registers and the user usually never
   sees; a *plugin* is something the app shows you.
2. **The surfaces with no UI are the most useful and the least
   discoverable.**  A Finder Sync extension, a Spotlight importer, a
   local REST API — these do not need a window, which is exactly why
   nobody built a UI for them and exactly why a tunnel fits them.
3. **Two constraints from the catalogue shape the transport.**  Figma's
   plugins are short-lived and user-invoked, so a Figma surface cannot
   hold a socket; and Chrome's worker dies after thirty seconds, so no
   browser surface can hold one either.  **The socket lives outside the
   app**, and the app pushes into it when it is awake.
4. **The growing surfaces are agent surfaces.**  MCP's extensions
   framework, ACP, the agent plugin directories: the client side of this
   project is an agent far more often than it is a human.
5. **Nothing in the catalogue ships a tunnel.**  Every one of them is
   either in-process (a plugin), a local socket (a secret API), or a
   document protocol (MCP).  The gap between "in-process" and "over the
   internet", with a user in the middle who can see and revoke it, is
   the gap this project fills.
