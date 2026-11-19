# extension surfaces of apps

the research the design rests on. first written 2026-11-18.

question: *which apps let a user install something into them, and through what surface?* the answer decides the protocol, so it comes first.

everything in sections 1-4 is checked against a source named at the end of that section. section 6 lists surfaces that are known but weren't checked against a source in this pass; they're marked as such, and they're the ones to check before building anything on them.

---

## 1. six kinds of surface

an "extension surface" is any point where a third party can install code or a document that the host app will execute or read. the kinds differ, and the difference matters more than the list.

| # | kind | what the third party gets | examples |
|---|------|--------------------------|----------|
| 1 | **web extension** | sandboxed JS/TS bundle + permission list, run by a browser engine | Chrome, Edge, Firefox, Safari, Brave, Arc, Opera, Vivaldi |
| 2 | **host plugin API** | in-process or out-of-process module loaded by the app itself | VS Code, JetBrains, Zed, Neovim, Emacs, Sublime, Xcode, Unity, Godot, Blender, Houdini, Obsidian, Notion, Figma, Adobe |
| 3 | **system app extension** | separate bundle with its own sandbox, registered with the OS, usually no UI | Apple's extension points: Mail, Finder Sync, Xcode Source Editor, Content Blocker, Audio Unit, Share/Action, Intents, Spotlight Import, Quick Look, Notification Service, Network Extension, File Provider, Custom Keyboard, Contact/Call Directory, iMessage, Photo Editing, Widget |
| 4 | **secret API** | documented (or discoverable) interface with no installer at all | MailKit, AppleScript/Automation, Shortcuts, Audio Units, Postman's pre-request scripts, mitmproxy add-ons, Wireshark Lua, Home Assistant integrations, Homebridge, ioBroker, openHAB, systemd units, OCI hooks, Docker socket API, D-Bus, MPD, Chrome DevTools Protocol, DevTools extensions, Playwright/Puppeteer, macOS Accessibility APIs |
| 5 | **agent protocol** | JSON-RPC surface aimed at agents, not humans | MCP (tools, resources, prompts, Tasks, MCP Apps), ACP (Agent Client Protocol), Agent Skills, agent plugin directories |
| 6 | **the app's own marketplace** | the app's own plugin format, shipped through the app's own registry | Cursor plugins, the Agent Plugins standard, Claude Code plugins, JetBrains Marketplace, VS Code Marketplace, Open VSX, Flathub, Steam Workshop, Decky, Homebrew, WordPress, Atlassian, NVDA add-ons |

the taxonomy is the useful part: kinds 1 and 3 are sandboxed and user-installed; kind 4 is invisible to the user; kind 6 is growing fastest right now, and it's where the vendor gates access.

---

## 2. kind 1: web extensions, and what Manifest V3 changed

Chrome's MV3 is the platform every other kind borrows from.

* MV3 swaps the long-lived background page for a **service worker** that runs only when needed, and **removes remotely hosted code**; an extension can only execute JavaScript shipped inside its package. `webRequest` blocking is deprecated in favor of `declarativeNetRequest`.
  ([Chrome for Developers](https://developer.chrome.com/docs/extensions/develop/migrate/what-is-mv3))
* the **cross-browser manifest** is `background.scripts` *and* `background.service_worker` together: Chromium takes the service worker, Firefox + Safari take the scripts. Safari also honors `preferred_environment: ["service_worker"]`, but see §3. Safari supports both forms and uses `scripts` unless told otherwise.
  ([MDN](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json/background))
* `browser.*` is the standard namespace; **Chrome supports it from Chrome 148** (for every extension from Chrome 152).
  ([MDN, cross-browser guide](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Build_a_cross_browser_extension))
* Firefox and Safari return promises from every async API; Chrome does from Chrome 121.

### the fact that decides the client design

**an MV3 service worker is terminated after 30s of inactivity**, and a single event can't run longer than 5 minutes. from **Chrome 116**, *WebSocket traffic* counts as extension activity and resets that timer, but only traffic that's actually flowing: an idle socket doesn't keep the worker alive, and neither does a reconnect loop against a dead server (Chromium's position: an idle connection shouldn't keep the worker resident). documented pattern => **ping every 20s**.
([Chrome for Developers](https://developer.chrome.com/docs/extensions/how-to/web-platform/websockets),
[Chromium issue 40245674](https://issues.chromium.org/40245674))

ports from extension pages also keep the worker alive while open.

---

## 3. kind 3: apple app extensions, and the ones with no UI

apple's extension points are the best example of an extension surface hosted by the OS, and the one the user named.

**Mail.app extensions** are built with MailKit; 4 kinds:

* **compose**: validate recipients, add headers, supply a view controller;
* **action**: flag, color, move or archive a message as it arrives;
* **content blocker**: WebKit rules for the message view;
* **message security**: sign, encrypt, decrypt.

entry point is `MEExtension`; each capability has its own handler protocol. ([MailKit](https://www.developer.apple.com/documentation/mailkit),
[Build Mail app extensions](https://www.developer.apple.com/documentation/mailkit/build-mail-app-extensions),
[WWDC21 session 10168](https://developer.apple.com/videos/play/wwdc2021/10168/),
[Use Mail extensions in Mail on Mac](https://support.apple.com/guide/mail/use-mail-extensions-mlhla9a93cd5/mac))

other extension points **carry no interface at all**; apple's own docs say many app extensions "simply provide information for the system to use":

| extension point | what it does | UI |
|---|---|---|
| Finder Sync | badges, labels, context menus, toolbar button for synced folders | none |
| Xcode Source Editor | commands over a text buffer, invoked by Xcode | none (Xcode draws it) |
| Content Blocker | rules for WebKit | none |
| Audio Unit | generate or modify audio in any host | optional |
| Spotlight Import | index content the app owns | none |
| Notification Service | rewrite a notification payload | none |
| File Provider | expose a document repository | optional |
| Intents / App Intents | answer Siri and Shortcuts | optional |
| Network Extension | VPN, proxy, content filter | none |

([App Extensions overview](https://developer.apple.com/documentation/technologyoverviews/app-extensions.md),
[Finder Sync guide](https://developer.apple.com/library/archive/documentation/General/Conceptual/ExtensibilityPG/Finder.html),
[Xcode source editor extension](https://devstreaming-cdn.apple.com/videos/wwdc/2016/414k6h6qdzyihpowpaa/414/414_using_and_extending_the_xcode_source_editor.pdf))

macOS Sequoia 15.4.1 already carries **over 400 app extensions and plugins** managed by PlugInKit, and a lot of them are **not exposed in System Settings** at all; the user never sees a list. ([eclectic light](https://eclecticlight.co/2025/04/23/an-overview-of-app-extensions-and-plugins-in-macos-sequoia/))

**Safari web extensions** have 3 moving parts: a containing app, the JS, and a native app extension that mediates between them. the JS talks to the native part with `browser.runtime.sendNativeMessage` / `connectNative`; **Safari ignores the application id and always talks to the containing app's native extension**. content scripts can't message the native part at all.
([Apple: messaging between the app and JavaScript](https://developer.apple.com/documentation/safariservices/messaging-between-the-app-and-javascript-in-a-safari-web-extension))

2 measured Safari facts that shape the client:

* **an MV3 service worker never started on Safari 26.5.2**, including in apple's own converter; a background page ran immediately (WebKit bug 270750). ([extension.js field notes](https://extension.js.org/docs/implementation-guide/background))
* **WebSocket connections to `127.0.0.1` are broken in Safari**, and Safari unloads idle background contexts after ~30–45 s even with in-flight `sendNativeMessage` promises => replies can be silently dropped, and native ports get closed without being reconnected.
  ([safari-pilot field notes](https://github.com/RTinkslinger/safari-pilot/blob/main/safari-mv3-event-page-native-messaging-2026-04-17.md))

so in Safari the near end of a tunnel can't be a WebSocket from the extension. it has to be a native host, reached over native messaging, and the native host holds the socket. this research forces that design decision.

---

## 4. kinds 2 and 6: editor and app plugin systems, and which are growing

the measurement that matters most: **new software with modest real usage (1.5–10×) has increased in consumer and hobbyist ecosystems where listing is easy**: NVDA add-ons, Home Assistant integrations, Obsidian plugins, Firefox and Chrome extensions, Steam games. it has **not** increased in professional or heavily gatekept registries: Atlassian, WordPress, Flathub, Google Play, Decky. new software that *does* reach high usage is overwhelmingly AI tooling: agent integrations for Obsidian, Jira and Chrome; LLM desktop apps on Flathub; AI CLIs on Homebrew.
([where-is-the-software](https://github.com/sgoedecke/where-is-the-software/blob/main/WRITEUP.md))

same source's numbers: Obsidian plugins with ≥30k downloads in 60 days went from **0–2 per quarter to 2–4**, mostly AI-agent plugins (Claudian, Agent Client, Claude sidebar). Chrome's new extensions with ≥100k users are now *mostly AI/agent extensions*. VS Code's new extensions with ≥10k installs are **down** (157 → 81 → 60 per cohort).

### editors

* **VS Code** and its forks. Cursor installs from **Open VSX**, not the Microsoft Marketplace, through a marketplace proxy that runs malware + supply-chain checks; it has an **allowed-extensions list**, a **Marketplace Install Cooldown** that delays new versions, and signature verification on Enterprise. ([Agentdex on Cursor](https://agentdex.app/blog/cursor-ai-extensions/))
* **Antigravity** (Google) is a VS Code fork. its own docs describe *its* extensions going *into* other IDEs (VS Code, Visual Studio, JetBrains, Android Studio, Zed, Xcode, Eclipse), and its CLI ships an **ACP server** (`agy acp`) that editors launch to run the agent in their own chat panel. ([Antigravity IDE extensions](https://antigravity.google/docs/ide/extensions))
* **JetBrains** (Junie, Tabnine, Augment Code, CodeGPT), **Zed** (Rust, ACP support), **Warp** (Rust terminal, agentic), **Kiro** (AWS), **Neovim** (Lua; CodeCompanion, avante, sidekick), **Emacs** (ELisp; agent-shell, acp.el), **Xcode**, **Eclipse**, **Sublime**, **Obsidian**, **Notion**, **Figma**, **Blender**, **Unity**, **Godot**.
  ([agentgid: AI IDEs and editor agents](https://agentgid.com/categories/coding-ide/),
  [archestack ranking](https://archestack.com/best-of-vibe-coding/coding-agents/))

### kind 5: agent protocols, the closest existing thing

* **MCP** now has a formal **Extensions framework**: extensions are identified by **reverse-DNS ids** (`io.modelcontextprotocol/ui`), negotiated through an `extensions` map on client and server capabilities, live in their own `ext-*` repos, and version independently. **MCP Apps** (SEP-1865) declares UI resources with the **`ui://` URI scheme** and gives tools a **visibility** of `["model", "app"]`; the tool is callable by the UI but hidden from the model's tool list. **Tasks** (SEP-1724) turns long-running work into a handle the client drives with `tasks/get`, `tasks/update`, `tasks/cancel`.
  ([MCP extensions overview](https://modelcontextprotocol.io/extensions/overview),
  [MCP Apps spec](https://github.com/modelcontextprotocol/ext-apps/blob/v2.0.3/specification/2026-01-26/apps.mdx),
  [2026-07-28 release candidate](https://blog.modelcontextprotocol.io/posts/2026-07-28-release-candidate/),
  [MCP Apps patterns](https://apps.extensions.modelcontextprotocol.io/api/documents/Patterns.html))
* **ACP** (Agent Client Protocol) is how an editor launches an agent process and drives it; Zed, Neovim, Emacs and Antigravity's CLI all speak it.

we take 3 ideas from MCP and refuse 1. reverse-DNS extension id, `ui://` scheme, per-tool visibility => all map onto this design directly. the JSON-RPC request/response shape doesn't: a tunnel has to survive a worker that's killed every 30s, and request/response with an open transaction is the wrong shape for that.

---

## 5. surfaces that hide it

the user was right that Antigravity hides it, and it's not alone.

* **Antigravity IDE** documents only its own extensions going into *other* IDEs. third-party extensions of the standalone IDE aren't part of the documented surface.
* **Cursor** gates with an allowed-extensions list, a marketplace install cooldown and a proxy registry.
* **apple's own app extensions**: 400+, most not listed in System Settings, all managed by PlugInKit behind the scenes.
* **apps with agent features but no extension surface**: the assistant in an app is a *feature*, and a feature gives third parties no way in. this project exists to fill that gap.

---

## 6. the surfaces checked in the second pass

the first pass listed these as known but unchecked. they're checked now, one row each, and each carries a source. what's confirmed is that the surface exists and what the third party gets; none of them is measured the way §2 measures MV3.

### audio plugins

| surface | what the third party gets | source |
|---|---|---|
| **Audio Units** (Logic Pro, GarageBand, every AU host) | a plug-in binary the host loads into its audio graph, with or without UI | [WWDC15 408 — Audio Unit Extensions](https://developer.apple.com/videos/play/wwdc2015/408/) |
| **VST3** (Ableton, Bitwig, Reaper, Cubase, JUCE, Max/MSP) | a plug-in module the host loads into its audio graph | [Steinberg — Building VST Plug-ins](https://developer.steinberg.help/display/VST/Building+VST+Plug-ins) |

### scripts inside a tool

| surface | what the third party gets | source |
|---|---|---|
| **Postman** | pre-request and test scripts, JavaScript in the sandbox | [Intro to scripts](https://learning.postman.com/docs/tests-and-scripts/write-scripts/intro-to-scripts/), [Test scripts](https://learning.postman.com/docs/tests-and-scripts/write-scripts/test-scripts/) |
| **mitmproxy** | an add-on, a Python class the proxy loads | [mitmproxy — Addons](https://docs.mitmproxy.org/stable/addons-overview/) |
| **Wireshark** | a Lua tap or dissector, loaded at run time | [Wireshark — Lua](https://wiki.wireshark.org/Lua) |

### the desktop automation layer

| surface | what the third party gets | source |
|---|---|---|
| **AppleScript / Automation** | scriptable access to most apps, behind an Automation consent prompt | [AppleScript Language Guide](https://developer.apple.com/library/archive/documentation/AppleScript/Conceptual/AppleScriptLangGuide/introduction/ASLR_intro.html) |
| **Shortcuts** | a library of user-made actions; App Intents is the code side | [Shortcuts User Guide](https://support.apple.com/guide/shortcuts/welcome/ios), [App Intents](https://developer.apple.com/documentation/appintents) |
| **Keyboard Maestro** | macros the editor runs | [Keyboard Maestro help](https://www.keyboardmaestro.com/help/) |
| **Alfred** | workflows: triggers, actions, scripts | [Alfred — Workflows](https://www.alfredapp.com/help/workflows/) |
| **Raycast** | extensions, distributed through Raycast's own store | [Raycast Developers](https://developers.raycast.com/) |
| **Stream Deck** | plug-ins the driver loads, one per key or panel | [Elgato SDK](https://docs.elgato.com/sdk/) |
| **macOS Accessibility** | the AXUIElement API: read and drive the UI of other apps | [AXUIElement](https://developer.apple.com/documentation/applicationservices/axuielement) |

### the home and the machine

| surface | what the third party gets | source |
|---|---|---|
| **Home Assistant** | an integration, installed from the UI | [integration manifest](https://developers.home-assistant.io/docs/creating_integration_manifest/) |
| **Homebridge** | a platform plug-in, in Node | [Homebridge Developers](https://developers.homebridge.io/) |
| **ioBroker** | an adapter | [ioBroker — adapter development](https://github.com/ioBroker/ioBroker.docs/tree/master/docs/en/dev) |
| **openHAB** | an add-on: bindings, transformations, persistence | [openHAB — Add-ons](https://www.openhab.org/docs/configuration/addons/) |
| **Docker** | the legacy builder and registry APIs, and the socket itself | [Docker — Extend Docker](https://docs.docker.com/engine/extend/) |
| **OCI runtime hooks** | a hook program the runtime runs at create/start/stop | [OCI Runtime Spec — hooks](https://github.com/opencontainers/runtime-spec/blob/main/config.md) |
| **systemd** | units, drop-ins, generators: the machine's own service surface | [systemd.unit(5)](https://www.freedesktop.org/software/systemd/man/systemd.unit.html) |

### the apps with communities around them

| surface | what the third party gets | source |
|---|---|---|
| **Discord** | bots, activities, the gateway API — a remote app, not a plug-in | [Developer Portal](https://discord.com/developers/docs/intro) |
| **Slack** | apps: bots, slash commands, webhooks | [Slack API](https://api.slack.com/) |
| **Adobe UXP** | a plug-in for Photoshop and the Creative Cloud apps | [Adobe — UXP](https://developer.adobe.com/photoshop/uxp/) |
| **OBS** | Lua/Python scripts and native plug-ins | [Scripting guide](https://obsproject.com/kb/scripting-guide), [Plugins guide](https://obsproject.com/kb/plugins-guide) |
| **Kodi** | add-ons of several kinds: plugins, scripts, services, scrapers | [Kodi — Add-on development](https://kodi.wiki/view/Add-on_development) |
| **Steam Workshop** | user-made content, published through Valve's pipeline | [Steamworks — Workshop](https://partner.steamgames.com/doc/features/workshop) |
| **Decky** | plug-ins for the Steam Deck's own UI | [Decky Loader](https://github.com/SteamDeckHomebrew/decky-loader) |

### the interfaces a client drives

| surface | what the third party gets | source |
|---|---|---|
| **D-Bus** | methods and signals on the session or system bus | [D-Bus tutorial](https://dbus.freedesktop.org/doc/dbus-tutorial.html) |
| **MPD** | a text protocol over TCP or a socket | [MPD protocol](https://www.musicpd.org/doc/html/protocol.html) |
| **TouchDesigner** | Python and C++ extensions of the operator family | [TouchDesigner — Python](https://docs.derivative.ca/Python) |
| **Chrome DevTools Protocol** | full control of a browser, over a WebSocket | [CDP](https://chromedevtools.github.io/devtools-protocol/) |
| **DevTools extensions** | a panel inside the DevTools front end | [Extending DevTools](https://developer.chrome.com/docs/extensions/how-to/devtools/extend-devtools) |
| **Playwright** | a driver that owns a browser the way a test harness does | [Playwright API](https://playwright.dev/docs/api/class-playwright) |
| **Puppeteer** | the same, over the DevTools Protocol | [Puppeteer](https://pptr.dev/) |

**what the second pass says.** the machine-and-home rows and the interfaces rows are all *secret* in the sense of §1 kind 4: a socket, a protocol, a bus. the desktop-automation rows are the ones a user installs and then forgets. nothing here changes §7; it closes the gaps the first pass left open.

---

## 7. what the research forces into the design

1. the near end of a tunnel is an extension, living inside the app's own sandbox with the app's own permissions. that's the only place the app's state is visible at all.
2. the near end can't be the same code on Chromium and Safari. Chromium: service worker + WebSocket, pinged every 20 s, because the worker dies after 30 s of silence. Safari: a background *page* (`background.scripts`, because the service worker never starts) talking to a **native host** over native messaging, because localhost WebSockets are broken and the background context gets unloaded anyway. the native host holds the socket.
3. the user sets a tunnel's scope at the extension, because that's where the consent surface already exists (Chrome's permission prompt, Safari's extension settings, Mail's per-extension toggle).
4. a schema has to be named, not implied. MCP's reverse-DNS ids and `ui://` scheme are the precedent; a tunnel needs both: an id for the schema, a scheme for the address.
5. nothing can depend on the far end being the app. the app sits behind the extension; the other end of the tunnel is a *client* (CLI, IDE, agent) that never sees the app's process.
6. the tunnel has to survive the client vanishing and the extension vanishing => resume by event id, replay from a ring buffer, and a ping that doesn't lie about liveness.

---

## sources

* [Chrome for Developers: What is Manifest V3](https://developer.chrome.com/docs/extensions/develop/migrate/what-is-mv3)
* [Chrome for Developers: WebSockets in service workers](https://developer.chrome.com/docs/extensions/how-to/web-platform/websockets)
* [Chromium issue 40245674: web sockets extend worker lifetime](https://issues.chromium.org/40245674)
* [MDN: background](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json/background)
* [MDN: build a cross-browser extension](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Build_a_cross-browser_extension)
* [extension.js: background scripts and MV3 service workers](https://extension.js.org/docs/implementation-guide/background)
* [Apple: App Extensions overview](https://developer.apple.com/documentation/technologyoverviews/app-extensions.md)
* [Apple: MailKit](https://www.developer.apple.com/documentation/mailkit) · [Build Mail app extensions](https://www.developer.apple.com/documentation/mailkit/build-mail-app-extensions) · [WWDC21 10168](https://developer.apple.com/videos/play/wwdc2021/10168/)
* [Apple: Use Mail extensions in Mail on Mac](https://support.apple.com/guide/mail/use-mail-extensions-mlhla9a93cd5/mac)
* [Apple: Finder Sync extension guide](https://developer.apple.com/library/archive/documentation/General/Conceptual/ExtensibilityPG/Finder.html)
* [Apple: messaging between the app and JavaScript in a Safari web extension](https://developer.apple.com/documentation/safariservices/messaging-between-the-app-and-javascript-in-a-safari-web-extension)
* [safari-pilot: MV3 event page and native messaging field notes](https://github.com/RTinkslinger/safari-pilot/blob/main/safari-mv3-event-page-native-messaging-2026-04-17.md)
* [eclectic light: app extensions and plugins in macOS Sequoia](https://eclecticlight.co/2025/04/23/an-overview-of-app-extensions-and-plugins-in-macos-sequoia/)
* [Antigravity: IDE extensions](https://antigravity.google/docs/ide/extensions) · [Antigravity enterprise](https://antigravity.google/docs/enterprise)
* [agentgid: AI IDEs and editor agents](https://agentgid.com/categories/coding-ide/)
* [Agentdex: Cursor AI extensions](https://agentdex.app/blog/cursor-ai-extensions/)
* [archestack: coding agents and AI editors](https://archestack.com/best-of-vibe-coding/coding-agents/)
* [where-is-the-software: WRITEUP](https://github.com/sgoedecke/where-is-the-software/blob/main/WRITEUP.md)
* [uptime.bet: open source trends October 2026](https://blog.uptime.bet/open-source-trends-october-2026/)
* [MCP: extensions overview](https://modelcontextprotocol.io/extensions/overview)
* [MCP Apps specification](https://github.com/modelcontextprotocol/ext-apps/blob/v2.0.3/specification/2026-01-26/apps.mdx)
* [MCP: 2026-07-28 release candidate](https://blog.modelcontextprotocol.io/posts/2026-07-28-release-candidate/)
* [MCP Apps: patterns](https://apps.extensions.modelcontextprotocol.io/api/documents/Patterns.html)
