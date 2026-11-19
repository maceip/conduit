# The Apple Mail surface

Mail has **one** extension point, `com.apple.email.extension`.  What the
extension is allowed to do is chosen by the `MEExtensionCapabilities` key
of the extension's `Info.plist`: `MEContentBlocker` gives the content
blocker, `MEComposeSessionHandler` gives the compose handler,
`MEMessageActionHandler` gives the action handler, and
`MESecuritySessionHandler` gives the security handler.  Apple's own list
of what Mail takes is "compose, action, content blocker, or security
extensions for Mail".

This surface takes two of the four: the compose window and the message
Mail downloads.

| the part | the file |
|---|---|
| the declaration | `surface.json` |
| the manifest of the extension | `ConduitMail.appex/Info.plist` |
| the principal class | `ConduitMail.appex/ConduitMailExtension.swift` |
| the compose handler | `ConduitMail.appex/ConduitMailCompose.swift` |
| the action handler | `ConduitMail.appex/ConduitMailAction.swift` |
| the client of the near end | `ConduitMail.appex/NearEnd.swift` |

## What it publishes

The message being composed — its subject, its recipients, an excerpt of
its body — under the schema `dev.conduit.mail.message/v1`, and the message
Mail downloads — its subject, who it is from, when it came — under the
same schema.  The payload says which of the two it is, in `kind`.

The extension publishes in plain JSON to the near end.  It holds no key,
it opens no socket to the broker, and it seals nothing: the near end does
the sealing, and the broker only ever sees the sealed form.

The publish is a frame of the tunnel, with a plaintext payload:
`{"t":"publish","s":"dev.conduit.mail.message/v1","c":"messages","p":{…}}`.
There is no second vocabulary on that socket.

The extension does not wait for the tunnel to agree its keys.  The near
end holds what is published until the two ends have met, and sends it
then; the waiting happens once, in the near end, and not in every
plugin in every language.  The near end says the tunnel is up with the
welcome frame, and complains with an error frame.

## What the user has to do

1. **Build it.**  Put `ConduitMail.appex` in an Xcode project as an
   app-extension target (a *Mail Extension* target), with the host
   application embedding it.  The target needs `MailKit.framework`, a
   bundle identifier of your own (`dev.conduit.mail.ConduitMail` is a
   reasonable one), and a development team: an unsigned extension is
   refused by PlugInKit.
2. **Sign it.**  Let Xcode sign the extension ("Automatically manage
   signing"), or sign it yourself:
   `codesign --force --deep --sign "Developer ID Application: …" ConduitMail.appex`.
   Check it with `codesign --verify --strict --verbose=2 ConduitMail.appex`.
3. **Register it with PlugInKit.**  Running the host app once registers
   the extension.  To do it by hand:
   `pluginkit -a -v /path/to/ConduitMail.appex`, and to see what Mail
   knows about it: `pluginkit -m -v -p com.apple.email.extension`.
   (`smpluginkit` is the tool for *system* extensions; a Mail extension is
   a PlugInKit plugin, so `pluginkit` is the one that knows it.)
4. **Turn it on in Mail.**  Mail → Settings → Extensions → *Conduit Mail*
   → tick the box.  On older systems it is Mail → Preferences → General →
   *Manage Extensions*.
5. **Point it at the near end.**  The extension looks for the socket in
   `CONDUIT_SOCKET`, then in the shared defaults of the app group
   `group.dev.conduit.mail` under the key `nearEndSocket`, and then at
   `/tmp/conduit-mail.sock`.  The near end must be running with the same
   path, and the path has to be one the sandboxed extension can open — a
   shared app-group container is the usual answer.

## The MailKit API it is written against

The extension is written against these members of MailKit, as Apple
documents them today:

| what | the API |
|---|---|
| the principal class | `MEExtension`, with `handler(for:)` and `handlerForMessageActions()` |
| the compose handler | `MEComposeSessionHandler`, with `mailComposeSessionDidBegin(_:)` and `mailComposeSessionDidEnd(_:)` |
| the action handler | `MEMessageActionHandler`, with `decideAction(for:completionHandler:)` |
| the message being composed | `MEComposeSession.mailMessage` and `MEComposeSession.composeContext` |
| the message Mail downloads | `MEMessage.subject`, `.fromAddress`, `.allRecipientAddresses`, `.rawData`, `.dateSent`, `.dateReceived` |
| an address | `MEEmailAddress.addressString` and `.rawString` |

`MEMessage.subject` is treated as a non-optional `String` and
`MEMessage.fromAddress` as a non-optional `MEEmailAddress`, which is how
Apple's current documentation has them; an SDK that still has them as
optionals is the one thing that would have to be adjusted.

## What was verified

**Nothing was compiled and nothing was run.**  This machine has no macOS,
no Xcode, no Swift toolchain and no Mail, so the four Swift files and
the `Info.plist` here have never been parsed by a compiler, never been
signed, never been registered with PlugInKit, and never been loaded by
Mail.  What was written was written by hand against Apple's published
MailKit API (`MEExtension`, `MEComposeSessionHandler`,
`MEMessageActionHandler`, `MEComposeSession.mailMessage`, `MEMessage`), and
the protocol against the contract in `docs/surface-contract.md` and the
Node client in `clients/native/near-end.js`.

To run it, on a Mac with Xcode:

* `xcodebuild -scheme ConduitMail -destination 'platform=macOS' build`
  to compile it,
* `pluginkit -a -v <the built appex>` and the Mail toggle above to make
  Mail load it,
* a near end running with `surfaces/apple-mail/surface.json`, and
* a `check.mjs` in this directory that drives the *same* publishing code
  against a live broker and a live near end, the way
  `surfaces/vscode/check.mjs` does.  There is no such check here: it
  would have to run the Swift, and Swift cannot run here.
