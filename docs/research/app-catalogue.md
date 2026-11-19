# catalogue: apps that ship an extension surface

which apps let a user install something into them, and what the surface is. first written 2026-11-18.

the companion doc, [`extension-surfaces.md`](extension-surfaces.md), sorts these surfaces by *kind* and says why kind matters. this one is the catalogue: 1 row per app; surface, what the third party gets, whether the user can see it.

last column:

* **listed** => the app shows the extension to the user by name, usually with a toggle.
* **hidden** => the surface exists and is user-installable, but the app doesn't surface it; the user finds it in a menu, a settings pane, or not at all.
* **secret** => no installer at all. the surface is an interface; knowing it exists is the only barrier.

every row carries a source. a row that cannot be sourced does not belong in this catalogue.

---

## the top twenty apps

the twenty apps with the most people able to install something into them. the order is reach — how many people use the app *and* can install into it — and not a measured ranking; the only measured numbers this document quotes are the ones [`extension-surfaces.md`](extension-surfaces.md) §4 takes from where-is-the-software. where one store serves a suite (Microsoft 365, Google Workspace), the suite is one row. the sections below go deeper per family, and the secret surfaces have their own table at the end.

| # | app | surface | what the third party gets | visible? | source |
|---|---|---|---|---|---|
| 1 | **Google Chrome** | Manifest V3 web extension | a sandboxed JS bundle: a permission list, a service worker, host permissions; shipped through the Chrome Web Store or sideloaded | listed | [MV3](https://developer.chrome.com/docs/extensions/develop/migrate/what-is-mv3), [Chrome Web Store](https://chromewebstore.google.com/) |
| 2 | **Apple Safari** | Safari web extension, content blocker | JS plus a **native app extension** that mediates between the JS and the containing app; distributed with the app through the App Store | listed | [messaging](https://developer.apple.com/documentation/safariservices/messaging-between-the-app-and-javascript-in-a-safari-web-extension), [app extensions](https://developer.apple.com/documentation/technologyoverviews/app-extensions.md) |
| 3 | **Microsoft Edge** | Chromium Manifest V3 | the same bundle shape as Chrome, through the Edge Add-ons store; Microsoft reviews submissions | listed | [Edge extensions](https://learn.microsoft.com/en-us/microsoft-edge/extensions-chromium/) |
| 4 | **Mozilla Firefox** | WebExtension (MV3) | the same bundle shape, with the `browser.*` namespace; distributed through addons.mozilla.org | listed | [MDN — background](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json/background), [AMO](https://addons.mozilla.org/) |
| 5 | **Microsoft 365** (Word, Excel, PowerPoint, Outlook) | **Office Add-ins** (Office.js); Outlook also carries the legacy COM/VSTO add-ins | a web add-in — HTML and JS in a task pane or ribbon button — with an AppSource listing or a tenant catalog | listed | [Office Add-ins](https://learn.microsoft.com/en-us/office/dev/add-ins/) |
| 6 | **Google Workspace** (Docs, Sheets, Gmail) | **Workspace add-ons** (CardService), Gmail add-ons, Apps Script | a manifest plus script functions that Google hosts; published in the Google Workspace Marketplace | listed | [Workspace add-ons](https://developers.google.com/workspace/add-ons/overview), [Gmail add-ons](https://developers.google.com/gmail/add-ons), [Apps Script](https://developers.google.com/apps-script), [Marketplace](https://workspace.google.com/marketplace) |
| 7 | **Visual Studio Code** | the extension API | a Node or worker extension in the shared extension host; the API covers the editor, the workspace, the language servers, the debuggers | listed | [Extension Marketplace](https://code.visualstudio.com/docs/configure/extensions/extension-marketplace) |
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
| 19 | **Apple Xcode** | the source editor extension | commands over a text buffer; **Xcode draws the UI** — the extension has none | **hidden** | [WWDC16 414](https://devstreaming-cdn.apple.com/videos/play/wwdc/2016/414k6h6qdzyihpowpaa/414/414_using_and_extending_the_xcode_source_editor.pdf) |
| 20 | **Unity** | editor extensions, custom packages, Asset Store tools | C# editor code and packages, loaded by the editor; shipped through the Package Manager or the Asset Store | listed | [Extending the Editor](https://docs.unity3d.com/Manual/ExtendingTheEditor.html) |

**what the top twenty says.** nine of the twenty ship a web bundle: Chrome, Safari, Edge, Firefox, VS Code, Figma, Photoshop, Microsoft 365 and Google Workspace. five install nothing at all — Slack, Teams, Discord, Zoom and Salesforce are remote apps the user authorizes. the other six are mixed or in-process: a PHP plugin in the WordPress site, a JVM plugin in the JetBrains IDE, an app extension in Xcode, C# in the Unity editor, Shopify's injected UI bundles, and Atlassian's hosted Forge functions. the in-process ones are where a tunnel fits best: the state lives inside the app and nowhere else.

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
| **Sublime Text** | the plugin API, in Python | a plugin the editor loads | listed | [API reference](https://www.sublimetext.com/docs/api_reference.html) |
| **Eclipse** | the plug-in development environment (PDE) | a JVM plug-in, through the extension registry | listed | [Eclipse PDE](https://www.eclipse.org/pde/) |
| **Warp** | the terminal's own extensions | a block or command the terminal runs | listed | [Warp docs](https://docs.warp.dev/) |

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
| **Antigravity, the IDE itself** | plugins: `plugin.json`, skills, rules, hooks, MCP servers, loaded from `.agents/plugins/` or `~/.gemini/config/plugins/` | reusable skills, subagents, rules, lifecycle hooks, MCP servers | listed | [plugins](https://antigravity.google/docs/plugins), [build plugins](https://antigravity.google/docs/build-plugins) |
| **Cursor, Claude Code, the Agent Plugins standard** | plugin directories with a marketplace | packaged agent: commands, agents, hooks, MCP servers | listed | [Agentdex on Cursor](https://agentdex.app/blog/cursor-ai-extensions/), [Claude Code](https://docs.anthropic.com/en/docs/claude-code/overview), [Agent Plugins](https://agent-plugins.org/) |

## secret surfaces: no installer at all

these are what the user meant by "it can just be like a secret API or anything". none has an extension *store*; each is an interface you have to know about.

| surface | what it is | how it's reached | source |
|---|---|---|---|
| **Obsidian's local REST API** | HTTP API over the vault | `127.0.0.1:27124`, Bearer token | [the plugin](https://community.obsidian.md/plugins/rest-api) |
| **AppleScript / Automation** | scriptable access to most apps | `osascript`, or an app that asks for automation permission | [AppleScript Language Guide](https://developer.apple.com/library/archive/documentation/AppleScript/Conceptual/AppleScriptLangGuide/introduction/ASLR_intro.html) |
| **Shortcuts** | library of user-made actions | Shortcuts app | [Shortcuts User Guide](https://support.apple.com/guide/shortcuts/welcome/ios), [App Intents](https://developer.apple.com/documentation/appintents) |
| **Home Assistant** | device integrations | YAML or a custom component, installed from the UI | [integration manifest](https://developers.home-assistant.io/docs/creating_integration_manifest/) |
| **Docker socket API** | control of the container runtime | Unix socket | [Extend Docker](https://docs.docker.com/engine/extend/), [OCI hooks](https://github.com/opencontainers/runtime-spec/blob/main/config.md) |
| **Chrome DevTools Protocol** | full control of a browser | WebSocket on the debug port | [CDP](https://chromedevtools.github.io/devtools-protocol/) |
| **mitmproxy add-ons, Postman pre-request scripts, Wireshark Lua** | scripts inside a network tool | the tool's own loader | [mitmproxy](https://docs.mitmproxy.org/stable/addons-overview/), [Postman scripts](https://learning.postman.com/docs/tests-and-scripts/write-scripts/intro-to-scripts/), [Wireshark Lua](https://wiki.wireshark.org/Lua) |
| **D-Bus, MPD, Audio Units, VST3** | interfaces inside the desktop | system bus, protocol, plugin host | [D-Bus](https://dbus.freedesktop.org/doc/dbus-tutorial.html), [MPD](https://www.musicpd.org/doc/html/protocol.html), [WWDC15 408](https://developer.apple.com/videos/play/wwdc2015/408/), [VST3](https://developer.steinberg.help/display/VST/Building+VST+Plug-ins) |

## the apps with no extension surface

the question worth asking is not only which apps have one.  some of the
biggest apps have none at all, and it is worth saying so plainly rather
than inventing a surface for them.

| app | what is there | why it is not an extension surface | source |
|---|---|---|---|
| **WhatsApp** | the Business Platform's Cloud API and webhooks; Flow JSON a business publishes; WhatsApp Web is a web page | there is **nothing a user can install into WhatsApp**. everything third-party is server-side. a Flow is published by the business, not installed by the user. a browser extension that injects into WhatsApp Web is unofficial, and WhatsApp's terms forbid reverse engineering and third-party tools that function as the service. | [about the platform](https://developers.facebook.com/documentation/business-messaging/whatsapp/about-the-platform), [flows](https://developers.facebook.com/documentation/business-messaging/whatsapp/flows/guides/flowjson/), [terms of service](https://www.whatsapp.com/legal/terms-of-service) |
| **Messages, Telegram, Signal** | bots and accounts on the server side | a bot is an application talking to the service, not an extension inside the app | [Telegram bots](https://core.telegram.org/bots) |

so there is no WhatsApp surface in this project, and that is the finding, not a gap.  a WhatsApp surface would have to be a server-side integration: the same shape as the Slack and Discord surfaces, where the app is a remote one.

## what the catalogue says

1. visible surfaces are all called "plugins"; invisible ones are all "extensions" or "integrations". the naming gives it away: an *extension* is something the OS registers and the user usually never sees; a *plugin* is something the app shows you.
2. surfaces with no UI are the most useful and the least discoverable. a Finder Sync extension, a Spotlight importer, a local REST API: none needs a window => nobody built a UI for them, and a tunnel fits them.
3. 2 constraints from the catalogue shape the transport. Figma plugins are short-lived + user-invoked => a Figma surface can't hold a socket; Chrome's worker dies after 30s => no browser surface can hold one either. so the socket lives outside the app, and the app pushes into it when it's awake.
4. the growing surfaces are agent surfaces: MCP's extensions framework, ACP, agent plugin directories. the client side of this project is an agent far more often than a human.
5. the biggest apps are often the ones with no surface: WhatsApp has none at all, and its whole third-party story is server-side.  a surface is a thing the app *runs*, and WhatsApp runs nothing of yours.
6. nothing in the catalogue ships a tunnel. every entry is in-process (a plugin), a local socket (a secret API), or a document protocol (MCP). this project fills the gap between "in-process" and "over the internet", with a user in the middle who can see and revoke it.
