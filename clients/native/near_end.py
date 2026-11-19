"""The client of the near end: the half of a surface that lives in the app.

The near end is a separate process that holds the socket to the broker
and does the sealing.  The plugin talks to it over a Unix socket, in
plain JSON, one message at a time -- and the message is a frame of the
tunnel's own shape::

    plugin  ->  {"t":"publish","s":"dev.conduit.homeassistant.state/v1","c":"home","p":{...}}
    near end ->  {"t":"publish","s":"...","c":"...","seq":7,"p":{...}}

There is no second vocabulary.  The hop between a plugin and the near
end carries the frames the tunnel carries, and the only thing that
differs is that the payload of a published frame is plaintext: the near
end seals it under the key of the tunnel before it leaves the machine,
and the broker never sees it open.

The plugin does not wait for the tunnel to be up.  The near end holds
what it publishes until the two ends have agreed their keys, and sends
it then.  The waiting happens once, in the near end, and not in every
plugin in every language.
"""

from __future__ import annotations

import asyncio
import json
from typing import Any, Awaitable, Callable, Optional

#: The largest message the client will read, in bytes.
MAXIMUM = 1024 * 1024


class NearEndClient:
    """The client of a near end over a Unix socket."""

    def __init__(self, socket_path: str) -> None:
        self.socket_path = socket_path
        self._reader: Optional[asyncio.StreamReader] = None
        self._writer: Optional[asyncio.StreamWriter] = None
        self._handlers: list[Callable[[dict[str, Any]], Awaitable[None] | None]] = []
        self._task: Optional[asyncio.Task] = None
        self._channel = ""

    async def connect(self) -> "NearEndClient":
        """Open the socket to the near end."""
        self._reader, self._writer = await asyncio.open_unix_connection(self.socket_path)
        self._task = asyncio.create_task(self._read())
        return self

    def set_channel(self, channel: str) -> None:
        """The channel this plugin publishes on.

        The near end puts the channel of the surface on the frame
        anyway, so this is only for the plugin's own sake.
        """
        self._channel = channel

    async def publish(self, schema: str, payload: Any) -> None:
        """Publish what the app knows.

        The payload is JSON.  The near end seals it; the broker never
        sees it open.
        """
        await self._send(
            {
                "t": "publish",
                "s": schema,
                "c": self._channel,
                "seq": 0,
                "p": payload,
            }
        )

    async def connect_to_broker(self, where: dict[str, Any]) -> None:
        """Say where the broker is.

        The near end may be told this by a plugin that did the device
        flow: the browser knows the secret, and the near end holds the
        socket.
        """
        await self._send(
            {
                "t": "connect",
                "s": "",
                "c": self._channel,
                "seq": 0,
                "p": where,
            }
        )

    def on(self, handler: Callable[[dict[str, Any]], Awaitable[None] | None]) -> None:
        """Say what happens when the near end says something."""
        self._handlers.append(handler)

    async def close(self) -> None:
        """Close the socket."""
        if self._task is not None:
            self._task.cancel()
            self._task = None
        if self._writer is not None:
            self._writer.close()
            try:
                await self._writer.wait_closed()
            except (OSError, asyncio.CancelledError):
                pass
            self._writer = None

    async def _send(self, message: dict[str, Any]) -> None:
        if self._writer is None:
            raise RuntimeError("the client is not connected")
        body = json.dumps(message).encode("utf-8")
        self._writer.write(len(body).to_bytes(4, "big") + body)
        await self._writer.drain()

    async def _read(self) -> None:
        assert self._reader is not None
        while True:
            try:
                header = await self._reader.readexactly(4)
            except (asyncio.IncompleteReadError, ConnectionError):
                return
            length = int.from_bytes(header, "big")
            if length > MAXIMUM:
                return
            try:
                body = await self._reader.readexactly(length)
            except (asyncio.IncompleteReadError, ConnectionError):
                return
            try:
                frame = json.loads(body)
            except ValueError:
                continue
            for handler in self._handlers:
                result = handler(frame)
                if asyncio.iscoroutine(result):
                    await result
