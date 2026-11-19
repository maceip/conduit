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

Every row carries a source.  A row that cannot be sourced does not
belong in this catalogue.

---

## The top twenty apps

The twenty apps with the most people able to install something into
them.  The order is reach — how many people use the app *and* can
install into it — and not a measured ranking; the only measured numbers
this document quotes are the ones
[`extension-surfaces.md`](extension-surfaces.md) §4 takes from
where-is-the-software.  Where one store serves a suite (Microsoft 365,
Google Workspace), the suite is one row.  The sections below go deeper
per family, and the secret surfaces have their own table at the end.

| # | app | the surface | what the third party gets | visible? | source |
|---|---|---|---|---|---|
| 1 | **Google Chrome** | Manifest V3 web extension | a sandboxed JS bundle: a permission list, a service worker, host permissions; shipped through the Chrome Web Store or sideloaded | listed | [MV3](https://developer.chrome.com/docs/extensions/develop/migrate/what-is-mv3), [Chrome Web Store](https://chromewebstore.google.com/) |
| 2 | **Apple Safari** | Safari web extension, content blocker | JS plus a **native app extension** that mediates between the JS and the containing app; distributed with the app through the App Store | listed | [messaging](https://developer.apple.com/documentation/safariservices/messaging-between-the-app-and-javascript-in-a-safari-web-extension), [app extensions](https://developer.apple.com/documentation/technologyoverviews/app-extensions.md) |
| 3 | **Microsoft Edge** | Chromium Manifest V3 | the same bundle shape as Chrome, through the Edge Add-ons store; Microsoft reviews submissions | listed | [Edge extensions](https://learn.microsoft.com/en-us/microsoft-edge/extensions-chromium/) |
| 4 | **Mozilla Firefox** | WebExtension (MV3) | the same bundle shape, with the `browser.*` namespace; distributed through addons.mozilla.org | listed | [MDN — background](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json/background), [AMO](https://addons.mozilla.org/) |
| 5 | **Microsoft 365** (Word, Excel, PowerPoint, Outlook) | **Office Add-ins** (Office.js); Outlook also carries the legacy COM/VSTO add-ins | a web add-in — HTML and JS in a task pane or ribbon button — with an AppSource listing or a tenant catalog | listed | [Office Add-ins](https://learn.microsoft.com/en-us/office/dev/add-ins/) |
| 6 | **Google Workspace** (Docs, Sheets, Gmail) | **Workspace add-ons** (CardService), Gmail add-ons, and Apps Script | a manifest plus script functions that Google hosts; published in the Google Workspace Marketplace | listed | [Workspace add-ons](https://developers.google.com/workspace/add-ons/overview), [Gmail add-ons](https://developers.google.com/gmail/add-ons), [Apps Script](https://developers.google.com/apps-script), [Marketplace](https://workspace.google.com/marketplace) |
| 7 | **Visual Studio Code** | the extension API | a Node or worker extension in the shared extension host; the API covers the editor, the workspace, the language servers and the debuggers | listed | [Extension Marketplace](https://code.visualstudio.com/docs/configure/extensions/extension-marketplace) |
| 8 | **Slack** | the app platform: bots, slash commands, webhooks, Workflow Builder steps | a remote app over the Web API and Socket Mode — no code runs inside Slack | listed | [Slack API](https://api.slack.com/) |
| 9 | **Microsoft Teams** | Teams apps: tabs, bots, message extensions, connectors | a manifest package (the app package), sideloaded or from the Teams store; the app itself runs remotely | listed | [Teams platform](https://learn.microsoft.com/en-us/microsoftteams/platform/overview) |
| 10 | **Discord** | bots, activities, slash commands | a remote app over the gateway and HTTP API — the surface is a remote app, not a plug-in | listed | [Developer Portal](https://discord.com/developers/docs/intro) |
| 11 | **Zoom** | apps and the Meeting SDK | a remote app, or a native SDK client that joins the meeting; distributed through the Zoom Marketplace | listed | [Zoom developers](https://developers.zoom.us/docs/) |
| 12 | **Figma** | the Plugin API, widgets | a JS bundle in a sandbox, **short-lived and always user-invoked** — "it's not possible to build plugins that run in the background"; listed on Figma Community after approval | listed, after approval | [Plugin API](https://developers.figma.com/docs/plugins/) |
| 13 | **Adobe Photoshop** (Creative Cloud) | **UXP** plug-ins (the legacy surface was CEP) | a JS bundle with a manifest and scoped Photoshop APIs; distributed through Adobe's marketplace or sideloaded | listed | [Adobe UXP](https://developer.adobe.com/photoshop/uxp/) |
| 14 | **WordPress** | plugins and themes, per site | a PHP plugin, installed in the site; the wordpress.org directory is the store | listed | [Plugin handbook](https://developer.wordpress.org/plugins/) |
| 15 | **Shopify** | apps and **app extensions** (admin, checkout, customer accounts) | a remote app over OAuth, plus UI extension bundles that Shopify injects into its own screens | listed | [Apps](https://shopify.dev/docs/apps), [App extensions](https://shopify.dev/docs/apps/build/app-extensions) |
| 16 | **Atlassian** (Jira, Confluence) | **Forge** apps (hosted by Atlassian) and Connect (remote iframes) | a manifest plus functions Atlassian runs, or a framed app; distributed through the Atlassian Marketplace | listed | [Forge](https://developer.atlassian.com/platform/forge/) |
| 17 | **Salesforce** | Lightning components, Apex, managed packages | a package installed into an org; the store is the AppExchange | listed | [AppExchange](https://www.salesforce.com/appexchange/) |
| 18 | **JetBrains IDEs** (IntelliJ, PyCharm, …) | the plugin platform | a JVM plugin loaded by the IDE | listed | [JetBrains Marketplace docs](https://plugins.jetbrains.com/docs/) |
| 19 | **Apple Xcode** | the source editor extension | commands over a text buffer; **Xcode draws the UI** — the extension has none | **hidden** | [WWDC16 414](https://devstreaming-cdn.apple.com/videos/wwdc/2016/414k6h6qdzyihpowpaa/414/414_using_and_extending_the_xcode_source_editor.pdf) |
| 20 | **Unity** | editor extensions, custom packages, Asset Store tools | C# editor code and packages, loaded by the editor; shipped through the Package Manager or the Asset Store | listed | [Extending the Editor](https://docs.unity3d.com/Manual/ExtendingTheEditor.html) |

**What the top twenty says.**  Nine of the twenty ship a web bundle:
Chrome, Safari, Edge, Firefox, VS Code, Figma, Photoshop, Microsoft 365
and Google Workspace.  Five install nothing at all — Slack, Teams,
Discord, Zoom and Salesforce are remote apps the user authorizes.  The
other six are mixed or in-process: a PHP plugin in the WordPress site,
a JVM plugin in the JetBrains IDE, an app extension in Xcode, C# in the
Unity editor, Shopify's injected UI bundles, and Atlassian's hosted
Forge functions.  The in-process ones are where a tunnel fits best: the
state lives inside the app and nowhere else.

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
| **Sublime Text** | the plugin API, in Python | a plugin the editor loads | listed | [API reference](https://www.sublimetext.com/docs/api_reference.html) |
| **Eclipse** | the plug-in development environment (PDE) | a JVM plug-in, through the extension registry | listed | [Eclipse PDE](https://www.eclipse.org/pde/) |
| **Xcode** | the surface is the source editor extension, above; Xcode itself has no third-party plug-in marketplace | — | hidden | [WWDC16 414](https://devstreaming-cdn.apple.com/videos/wwdc/2016/414k6h6qdzyihpowpaa/414/414_using_and_extending_the_xcode_source_editor.pdf) |
| **Warp** | the terminal's own extensions | a block or command the terminal runs | listed | [Warp docs](https://docs.warp.dev/) |

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
| **Cursor, Claude Code, the Agent Plugins standard** | plugin directories with a marketplace | a packaged agent, with commands, agents, hooks and MCP servers | listed | [Agentdex on Cursor](https://agentdex.app/blog/cursor-ai-extensions/), [Claude Code](https://docs.anthropic.com/en/docs/claude-code/overview), [Agent Plugins](https://agent-plugins.org/) |

## The secret surfaces — no installer at all

These are the ones the user was reaching for with "it can just be like a
secret API or anything".  None of them has an extension *store*; each is
an interface you have to know about.

| surface | what it is | how it is reached | source |
|---|---|---|---|
| **Obsidian's local REST API** | an HTTP API over the vault | `127.0.0.1:27124`, Bearer token | [the plugin](https://community.obsidian.md/plugins/rest-api) |
| **AppleScript / Automation** | scriptable access to most apps | `osascript`, or an app that asks for automation permission | [AppleScript Language Guide](https://developer.apple.com/library/archive/documentation/AppleScript/Conceptual/AppleScriptLangGuide/introduction/ASLR_intro.html) |
| **Shortcuts** | a library of user-made actions | the Shortcuts app | [Shortcuts User Guide](https://support.apple.com/guide/shortcuts/welcome/ios), [App Intents](https://developer.apple.com/documentation/appintents) |
| **Home Assistant** | integrations for devices | YAML or a custom component, installed from the UI | [integration manifest](https://developers.home-assistant.io/docs/creating_integration_manifest/) |
| **The Docker socket API** | control of the container runtime | a Unix socket | [Extend Docker](https://docs.docker.com/engine/extend/), [OCI hooks](https://github.com/opencontainers/runtime-spec/blob/main/config.md) |
| **The Chrome DevTools Protocol** | control of a browser, fully | a WebSocket on the debug port | [CDP](https://chromedevtools.github.io/devtools-protocol/) |
| **mitmproxy add-ons, Postman pre-request scripts, Wireshark Lua** | scripts inside a network tool | the tool's own loader | [mitmproxy](https://docs.mitmproxy.org/stable/addons-overview/), [Postman scripts](https://learning.postman.com/docs/tests-and-scripts/write-scripts/intro-to-scripts/), [Wireshark Lua](https://wiki.wireshark.org/Lua) |
| **D-Bus, MPD, Audio Units, VST3** | interfaces inside the desktop | the system bus, the protocol, the plugin host | [D-Bus](https://dbus.freedesktop.org/doc/dbus-tutorial.html), [MPD](https://www.musicpd.org/doc/html/protocol.html), [WWDC15 408](https://developer.apple.com/videos/play/wwdc2015/408/), [VST3](https://developer.steinberg.help/display/VST/Building+VST+Plug-ins) |

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
