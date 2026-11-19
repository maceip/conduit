# The Discord surface

A Discord bot is a remote app: it connects to the Gateway for the
real-time events, and to the HTTP API for the rest
([bots](https://docs.discord.com/developers/bots/overview),
[application commands](https://docs.discord.com/developers/interactions/application-commands),
[OAuth2](https://docs.discord.com/developers/topics/oauth2)).

The bot holds no key and opens no socket to the broker.  It publishes
what Discord says, in plain JSON, to the near end, which does the
sealing.

| the part | the file |
|---|---|
| the configuration of the bot | `bot.json` |
| the bot | `bot.js` |
| what a message is | `publish.js` |
| the declaration | `surface.json` |
| the check | `check.mjs` |

## Discord has no manifest file

There is no manifest for a Discord bot: the bot is made in the
Developer Portal, and what Discord reads is the application behind it.
So there is no manifest here either.  `bot.json` is the bot's own
configuration — the application id, the intents, the install URL — and
Discord never sees it.

`package.json` is here only to tell Node that the files in this
directory are CommonJS.

## What it publishes

The message of a channel, under the schema
`dev.conduit.discord.message/v1`: the guild it was said in, the
channel, who said it, and what was said.

The Conduit scope of the surface is `discord.messages.read`.  On the
Discord side that is the `MESSAGE_CONTENT` intent (`1 << 15`), with
`GUILDS` (`1 << 0`) beside it: `32769` in `bot.json`.

## The Discord side of it

`bot.js` opens the Gateway WebSocket, says hello with the intents of
`bot.json`, answers the heartbeats, and publishes each `MESSAGE_CREATE`
event.

The gateway is asked for over the HTTP API (`GET /gateway/bot`), which
answers with a URL of the documented shape
`wss://gateway.discord.gg/?v=10&encoding=json`.  Without a bot token
the documented endpoint is used as a placeholder, and the bot says so.

The slash command `/conduit` is registered through the HTTP API, at the
documented path `POST /applications/<application_id>/commands`, with
the bot token.  The bot is installed by the user with the OAuth2 URL of
`bot.json`:
`https://discord.com/oauth2/authorize?client_id=…&scope=bot%20applications.commands`.

## What was verified

**Verified on this machine.** `check.mjs` starts a real broker and a
real near end, runs `publish.js` — the same module `bot.js` imports —
and the client at the far end receives the message, opened.  The frame
that crossed the broker was not the message in the clear.

**Not verified: the Discord side.** The Discord side was never
connected to discord.com from this machine.  `bot.js` has never run
against Discord: no application, no bot token, no guild.  The machine
can reach discord.com — the unauthenticated `GET /api/v10/gateway`
answers `{"url":"wss://gateway.discord.gg"}` — but nothing was ever
authenticated.  So the Gateway handshake, the heartbeat and the
registration of the slash command are **not run**, and only the
publishing half of the surface is verified.
