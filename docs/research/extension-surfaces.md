# The extension surfaces of apps

**The research that the design rests on.**  First written 2026-11-18.

The question this document answers: *which apps let a user install something into
them, and what is the surface they do it through?*  The answer decides the
protocol, so it comes first.

Everything in the first four sections is checked against a source that is
named at the end of the section.  The fifth section is a list of surfaces
that are known but were not checked against a source in this pass — they are
marked as such, and they are the ones to check before anything is built on
them.

---

## 1. Six kinds of surface

An "extension surface" is any seam where a third party can install code or
a document that the host app will execute or read.  They are not the same
thing, and the difference matters more than the list does.

| # | Kind | What the third party gets | Examples |
|---|------|--------------------------|----------|
| 1 | **Web extension** | A sandboxed JS/TS bundle with a permission list, run by a browser engine | Chrome, Edge, Firefox, Safari, Brave, Arc, Opera, Vivaldi |
| 2 | **Host plugin API** | An in-process or out-of-process module loaded by the app itself | VS Code, JetBrains, Zed, Neovim, Emacs, Sublime, Xcode, Unity, Godot, Blender, Houdini, Obsidian, Notion, Figma, Adobe |
| 3 | **System app extension** | A separate bundle with its own sandbox, registered with the OS, usually no UI | Apple's extension points: Mail, Finder Sync, Xcode Source Editor, Content Blocker, Audio Unit, Share/Action, Intents, Spotlight Import, Quick Look, Notification Service, Network Extension, File Provider, Custom Keyboard, Contact/Call Directory, iMessage, Photo Editing, Widget |
| 4 | **Secret API** | A documented (or discoverable) interface with no installer at all | MailKit, AppleScript/Automation, Shortcuts, Audio Units, Postman's pre-request scripts, mitmproxy add-ons, Wireshark Lua, Home Assistant integrations, Homebridge, ioBroker, openHAB, systemd units, OCI hooks, Docker socket API, D-Bus, MPD, Chrome DevTools Protocol, DevTools extensions, Playwright/Puppeteer, macOS Accessibility APIs |
| 5 | **Agent protocol** | A JSON-RPC surface aimed at agents rather than at humans | MCP (tools, resources, prompts, Tasks, MCP Apps), ACP (Agent Client Protocol), Agent Skills, agent plugin directories |
| 6 | **Marketplace of the app itself** | The app's own plugin format, shipped through the app's own registry | Cursor plugins, the Agent Plugins standard, Claude Code plugins, JetBrains Marketplace, VS Code Marketplace, Open VSX, Flathub, Steam Workshop, Decky, Homebrew, WordPress, Atlassian, NVDA add-ons |

The taxonomy is the useful part: kind 1 and kind 3 are **sandboxed and
installed by the user**; kind 4 is **invisible to the user**; kind 6 is the
one that is growing fastest right now and the one where the vendor gates
access.

---

## 2. Kind 1 — the web extensions, and what Manifest V3 changed

Chrome's MV3 is the platform every other kind borrows from.

* MV3 replaces the long-lived background page with a **service worker**
  that runs only when needed, and **removes remotely hosted code** — an
  extension may only execute JavaScript that ships inside its package.
  `webRequest` blocking is deprecated in favour of `declarativeNetRequest`.
  ([Chrome for Developers](https://developer.chrome.com/docs/extensions/develop/migrate/what-is-mv3))
* The **cross-browser manifest** is `background.scripts` *and*
  `background.service_worker` together: Chromium takes the service worker,
  Firefox and Safari take the scripts.  Safari also honours
  `preferred_environment: ["service_worker"]` — but see §3.  Safari
  supports both forms and uses `scripts` unless told otherwise.
  ([MDN](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json/background))
* The `browser.*` namespace is the standard; **Chrome supports it from
  Chrome 148** (and for every extension from Chrome 152).
  ([MDN, cross-browser guide](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Build_a_cross_browser_extension))
* Firefox and Safari return promises from every asynchronous API; Chrome
  does from Chrome 121.

### The fact that decides the client design

**An MV3 service worker is terminated after 30 seconds of inactivity**, and
a single event may not run longer than five minutes.  From **Chrome 116**,
*WebSocket traffic* counts as extension activity and resets that timer — but
only traffic that is actually flowing: an idle socket does not keep the
worker alive, and neither does a reconnect loop against a dead server
(Chromium's position is that an idle connection should not keep the worker
resident).  The documented pattern is a **ping every 20 seconds**.
([Chrome for Developers](https://developer.chrome.com/docs/extensions/how-to/web-platform/websockets),
[Chromium issue 40245674](https://issues.chromium.org/40245674))

Ports from extension pages also keep the worker alive while they are open.

---

## 3. Kind 3 — the Apple app extensions, and the ones with no UI

Apple's extension points are the best example of a *hosted-by-the-OS*
extension surface, and the one the user named.

**Mail.app extensions** are built with MailKit and come in four kinds:

* **compose** — validate recipients, add headers, supply a view controller;
* **action** — flag, colour, move or archive a message as it arrives;
* **content blocker** — WebKit rules for the message view;
* **message security** — sign, encrypt, decrypt.

The entry point is `MEExtension`, and each capability has its own handler
protocol.  ([MailKit](https://www.developer.apple.com/documentation/mailkit),
[Build Mail app extensions](https://www.developer.apple.com/documentation/mailkit/build-mail-app-extensions),
[WWDC21 session 10168](https://developer.apple.com/videos/play/wwdc2021/10168/),
[Use Mail extensions in Mail on Mac](https://support.apple.com/guide/mail/use-mail-extensions-mlhla9a93cd5/mac))

Other extension points that **carry no interface at all** — Apple's own
documentation says many app extensions "simply provide information for the
system to use":

| Extension point | What it does | UI |
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

macOS Sequoia 15.4.1 already carries **over 400 app extensions and plugins**
managed by PlugInKit, and a great many of them are **not exposed in System
Settings** at all — the user never sees a list.  ([eclectic light](https://eclecticlight.co/2025/04/23/an-overview-of-app-extensions-and-plugins-in-macos-sequoia/))

**Safari web extensions** are three moving parts: a containing app, the JS,
and a native app extension that mediates between them.  The JS talks to the
native part with `browser.runtime.sendNativeMessage` / `connectNative`; **Safari
ignores the application id and always talks to the containing app's native
extension**.  Content scripts cannot message the native part at all.
([Apple: messaging between the app and JavaScript](https://developer.apple.com/documentation/safariservices/messaging-between-the-app-and-javascript-in-a-safari-web-extension))

Two measured facts about Safari that shape the client:

* **An MV3 service worker never started on Safari 26.5.2**, including in
  Apple's own converter; a background page ran immediately (WebKit bug
  270750).  ([extension.js field notes](https://extension.js.org/docs/implementation-guide/background))
* **WebSocket connections to `127.0.0.1` are broken in Safari**, and Safari
  unloads idle background contexts after ~30–45 s even with in-flight
  `sendNativeMessage` promises, so replies can be silently dropped and
  native ports are closed without being reconnected.
  ([safari-pilot field notes](https://github.com/RTinkslinger/safari-pilot/blob/main/safari-mv3-event-page-native-messaging-2026-04-17.md))

**Therefore: in Safari the near end of a tunnel cannot be a WebSocket from
the extension.**  It has to be a native host, reached over native
messaging, and the native host holds the socket.  That is the design
decision this research forces.

---

## 4. Kind 2 and kind 6 — the editor and app plugin systems, and which are growing

The measurement that matters most: **new software with modest real usage
(1.5–10×) has increased in consumer and hobbyist ecosystems where listing
is easy** — NVDA add-ons, Home Assistant integrations, Obsidian plugins,
Firefox and Chrome extensions, Steam games.  It has **not** increased in
professional or heavily gatekept registries — Atlassian, WordPress, Flathub,
Google Play, Decky.  New software that *does* reach high usage is
overwhelmingly AI tooling: agent integrations for Obsidian, Jira and Chrome;
LLM desktop apps on Flathub; AI CLIs on Homebrew.
([where-is-the-software](https://github.com/sgoedecke/where-is-the-software/blob/main/WRITEUP.md))

The same source's numbers: Obsidian plugins with ≥30k downloads in 60 days
went from **0–2 per quarter to 2–4**, and they are mostly AI-agent plugins
(Claudian, Agent Client, Claude sidebar).  Chrome's new extensions with
≥100k users are now *mostly AI/agent extensions*.  VS Code's new extensions
with ≥10k installs are **down** (157 → 81 → 60 per cohort).

### The editors

* **VS Code** and its forks.  Cursor installs from **Open VSX**, not the
  Microsoft Marketplace, through a marketplace proxy that runs malware and
  supply-chain checks; it has an **allowed-extensions list**, a
  **Marketplace Install Cooldown** that delays new versions, and signature
  verification on Enterprise.  ([Agentdex on Cursor](https://agentdex.app/blog/cursor-ai-extensions/))
* **Antigravity** (Google) is a VS Code fork.  Its own documentation
  describes *its* extensions going *into* other IDEs — VS Code, Visual
  Studio, JetBrains, Android Studio, Zed, Xcode, Eclipse — and its CLI
  ships an **ACP server** (`agy acp`) that editors launch to run the agent
  in their own chat panel.  ([Antigravity IDE extensions](https://antigravity.google/docs/ide/extensions))
* **JetBrains** (Junie, Tabnine, Augment Code, CodeGPT), **Zed** (Rust,
  ACP support), **Warp** (Rust terminal, agentic), **Kiro** (AWS), **Neovim**
  (Lua; CodeCompanion, avante, sidekick), **Emacs** (ELisp; agent-shell,
  acp.el), **Xcode**, **Eclipse**, **Sublime**, **Obsidian**, **Notion**,
  **Figma**, **Blender**, **Unity**, **Godot**.
  ([agentgid: AI IDEs and editor agents](https://agentgid.com/categories/coding-ide/),
  [archestack ranking](https://archestack.com/best-of-vibe-coding/coding-agents/))

### Kind 5 — the agent protocols, which are the closest existing thing

* **MCP** now has a formal **Extensions framework**: extensions are
  identified by **reverse-DNS ids** (`io.modelcontextprotocol/ui`), are
  negotiated through an `extensions` map on client and server capabilities,
  live in their own `ext-*` repositories, and version independently.
  **MCP Apps** (SEP-1865) declares UI resources by the **`ui://` URI
  scheme**, and gives tools a **visibility** of `["model", "app"]` — the
  tool is callable by the UI but hidden from the model's tool list.
  **Tasks** (SEP-1724) makes long-running work a handle the client drives
  with `tasks/get`, `tasks/update`, `tasks/cancel`.
  ([MCP extensions overview](https://modelcontextprotocol.io/extensions/overview),
  [MCP Apps spec](https://github.com/modelcontextprotocol/ext-apps/blob/v2.0.3/specification/2026-01-26/apps.mdx),
  [2026-07-28 release candidate](https://blog.modelcontextprotocol.io/posts/2026-07-28-release-candidate/),
  [MCP Apps patterns](https://apps.extensions.modelcontextprotocol.io/api/documents/Patterns.html))
* **ACP** — the Agent Client Protocol — is how an editor launches an agent
  process and drives it; Zed, Neovim, Emacs and Antigravity's CLI all speak
  it.

**Three ideas in MCP are worth stealing and one is worth refusing.**  The
reverse-DNS extension id, the `ui://` scheme, and the per-tool visibility
map onto this design directly.  The JSON-RPC request/response shape does
not: a tunnel has to survive a worker that is killed every thirty seconds,
and a request/response protocol with an open transaction is the wrong shape
for that.

---

## 5. The surfaces that hide it

The user was right that Antigravity hides it, and it is not alone.

* **Antigravity IDE** documents only its own extensions going into *other*
  IDEs.  Third-party extensions of the standalone IDE are not part of the
  documented surface.
* **Cursor** gates with an allowed-extensions list, a marketplace install
  cooldown and a proxy registry.
* **Apple's own app extensions**: 400+ of them, most of them not listed in
  System Settings, all of them managed by PlugInKit behind the scenes.
* **Apps with agent features but no extension surface**: the assistant in
  an app is a *feature*, and a feature is not a seam.  This is the gap the
  project exists to fill.

---

## 6. The surfaces known but not checked here

Marked as unverified in this pass — check before building on them:

* Audio Units and VST3 as plugin surfaces (Logic Pro, GarageBand, Ableton,
  Bitwig, Reaper, Cubase, JUCE, Max/MSP).
* Postman's pre-request scripts, mitmproxy add-ons, Wireshark Lua.
* AppleScript and Automation, Shortcuts actions, Keyboard Maestro, Alfred,
  Raycast extensions, Stream Deck plugins.
* Home Assistant, Homebridge, ioBroker, openHAB, Docker/OCI hooks, systemd.
* Discord and Slack surfaces (bots are not extensions), Adobe CEP and UXP,
  OBS, Kodi, Steam Workshop, Decky.
* D-Bus, MPD, TouchDesigner, the Chrome DevTools Protocol, DevTools
  extensions, Playwright/Puppeteer, macOS Accessibility APIs.

---

## 7. What the research forces into the design

1. **The near end of a tunnel is an extension**, and the extension lives
   inside the app's own sandbox with the app's own permissions.  That is
   the only place where the app's state is visible at all.
2. **The near end cannot be the same code on Chromium and Safari.**
   Chromium: a service worker with a WebSocket, pinged every 20 s, because
   the worker dies after 30 s of silence.  Safari: a background *page*
   (`background.scripts`, because the service worker never starts), talking
   to a **native host** over native messaging, because localhost
   WebSockets are broken and the background context is unloaded anyway.
   The native host holds the socket.
3. **The scope of a tunnel is set by the user, at the extension**, because
   that is where the consent surface already exists (Chrome's permission
   prompt, Safari's extension settings, Mail's per-extension toggle).
4. **A schema has to be named, not implied** — MCP's reverse-DNS ids and
   `ui://` scheme are the precedent, and a tunnel needs both: an id for the
   schema and a scheme for the address.
5. **Nothing may depend on the far end being the app.**  The app is behind
   the extension; the other end of the tunnel is a *client* — a CLI, an
   IDE, an agent — that never sees the app's process.
6. **The tunnel must survive the client vanishing and the extension
   vanishing** — resume by event id, replay from a ring buffer, and a
   ping that is not a lie about liveness.

---

## Sources

* [Chrome for Developers — What is Manifest V3](https://developer.chrome.com/docs/extensions/develop/migrate/what-is-mv3)
* [Chrome for Developers — WebSockets in service workers](https://developer.chrome.com/docs/extensions/how-to/web-platform/websockets)
* [Chromium issue 40245674 — web sockets extend worker lifetime](https://issues.chromium.org/40245674)
* [MDN — background](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json/background)
* [MDN — build a cross-browser extension](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Build_a_cross-browser_extension)
* [extension.js — background scripts and MV3 service workers](https://extension.js.org/docs/implementation-guide/background)
* [Apple — App Extensions overview](https://developer.apple.com/documentation/technologyoverviews/app-extensions.md)
* [Apple — MailKit](https://www.developer.apple.com/documentation/mailkit) · [Build Mail app extensions](https://www.developer.apple.com/documentation/mailkit/build-mail-app-extensions) · [WWDC21 10168](https://developer.apple.com/videos/play/wwdc2021/10168/)
* [Apple — Use Mail extensions in Mail on Mac](https://support.apple.com/guide/mail/use-mail-extensions-mlhla9a93cd5/mac)
* [Apple — Finder Sync extension guide](https://developer.apple.com/library/archive/documentation/General/Conceptual/ExtensibilityPG/Finder.html)
* [Apple — messaging between the app and JavaScript in a Safari web extension](https://developer.apple.com/documentation/safariservices/messaging-between-the-app-and-javascript-in-a-safari-web-extension)
* [safari-pilot — MV3 event page and native messaging field notes](https://github.com/RTinkslinger/safari-pilot/blob/main/safari-mv3-event-page-native-messaging-2026-04-17.md)
* [eclectic light — app extensions and plugins in macOS Sequoia](https://eclecticlight.co/2025/04/23/an-overview-of-app-extensions-and-plugins-in-macos-sequoia/)
* [Antigravity — IDE extensions](https://antigravity.google/docs/ide/extensions) · [Antigravity enterprise](https://antigravity.google/docs/enterprise)
* [agentgid — AI IDEs and editor agents](https://agentgid.com/categories/coding-ide/)
* [Agentdex — Cursor AI extensions](https://agentdex.app/blog/cursor-ai-extensions/)
* [archestack — coding agents and AI editors](https://archestack.com/best-of-vibe-coding/coding-agents/)
* [where-is-the-software — WRITEUP](https://github.com/sgoedecke/where-is-the-software/blob/main/WRITEUP.md)
* [uptime.bet — open source trends October 2026](https://blog.uptime.bet/open-source-trends-october-2026/)
* [MCP — extensions overview](https://modelcontextprotocol.io/extensions/overview)
* [MCP Apps specification](https://github.com/modelcontextprotocol/ext-apps/blob/v2.0.3/specification/2026-01-26/apps.mdx)
* [MCP — 2026-07-28 release candidate](https://blog.modelcontextprotocol.io/posts/2026-07-28-release-candidate/)
* [MCP Apps — patterns](https://apps.extensions.modelcontextprotocol.io/api/documents/Patterns.html)
