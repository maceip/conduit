"""The client of the near end: the half of a surface that lives in the app.

The near end is a separate process that holds the socket to the broker
and does the sealing.  The plugin talks to it over a Unix socket, in
plain JSON, one message at a time:

    plugin  ->  {"op":"publish","schema":"dev.conduit.homeassistant.state/v1","payload":{...}}
    near end ->  {"op":"event","schema":"...","channel":"...","seq":7,"payload":{...}}

What the plugin publishes is plaintext on this socket.  What leaves the
machine is not: the near end seals it under the key of the tunnel, and
the broker never sees it open.
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

    async def connect(self) -> "NearEndClient":
        """Open the socket to the near end."""
        self._reader, self._writer = await asyncio.open_unix_connection(self.socket_path)
        self._task = asyncio.create_task(self._read())
        return self

    async def publish(self, schema: str, payload: Any) -> None:
        """Publish what the app knows.

        The payload is JSON.  The near end seals it; the broker never
        sees it open.
        """
        await self._send({"op": "publish", "schema": schema, "payload": payload})

    async def status(self) -> None:
        """Ask how the tunnel is."""
        await self._send({"op": "status"})

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
                message = json.loads(body)
            except ValueError:
                continue
            for handler in self._handlers:
                result = handler(message)
                if asyncio.iscoroutine(result):
                    await result
