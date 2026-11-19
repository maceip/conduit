# The Slack surface

A Slack app is remote: it runs on a server, and Slack sends it the
events over a WebSocket.  Socket Mode is the way to do that without
exposing a public HTTP Request URL
([app manifest](https://docs.slack.dev/reference/app-manifest/),
[Socket Mode](https://docs.slack.dev/apis/events-api/using-socket-mode/)).

The app holds no key and opens no socket to the broker.  It publishes
what Slack says, in plain JSON, to the near end, which does the
sealing.

| the part | the file |
|---|---|
| the Slack app manifest | `manifest.json` |
| the app | `app.js` |
| what a message is | `publish.js` |
| the declaration | `surface.json` |
| the check | `check.mjs` |

`package.json` is here only to tell Node that the files in this
directory are CommonJS.  It is not a Slack manifest: the manifest is
`manifest.json`.

## What it publishes

The message of a channel, under the schema `dev.conduit.slack.message/v1`:
the channel it was said in, who said it, what was said, and the message
it replies to when it is a reply.

The Conduit scope of the surface is `slack.messages.read`.  The Slack
scopes that carry it are named in the manifest, under
`oauth_config.scopes.bot`: `channels:history` and `channels:read` for
the public channels the bot is in, and `groups:history`, `groups:read`,
`im:history`, `im:read` and `mpim:history` for the private ones.  The
manifest subscribes to the `message.channels` event.

## The Slack side of it

`app.js` opens the Socket Mode WebSocket, acknowledges every envelope
Slack sends (Slack sends the ones that are not acknowledged again), and
publishes each `message` event and each `/conduit` slash command.

The endpoint Slack documents is `wss://app.slack.com/link`.  The real
connection is opened by calling `apps.connections.open` with an
app-level token (`SLACK_APP_TOKEN`, a token that starts with `xapp-`),
which answers with a URL of that shape.  Without a token the app uses
the documented endpoint as a placeholder, says so, and has nothing to
connect to.

## What was verified

**Verified on this machine.** `check.mjs` starts a real broker and a
real near end, runs `publish.js` — the same module `app.js` imports —
and the client at the far end receives the message, opened.  The frame
that crossed the broker was not the message in the clear.

**Not verified: the Slack side.** The Slack side was never connected to
slack.com from this machine.  `app.js` has never run against Slack: no
app was created, no app-level token and no bot token exist, and no
Socket Mode WebSocket was ever opened.  The machine can reach slack.com
— `POST /api/apps.connections.open` without a token answers
`{"ok":false,"error":"not_authed"}` — but nothing was ever authenticated.
So the Socket Mode handshake, the envelope acknowledgement and the
`/conduit` slash command are **not run**, and only the publishing half
of the surface is verified.
