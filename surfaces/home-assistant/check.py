#!/usr/bin/env python3
"""The check of the Home Assistant surface.

The publishing half of the integration — ``publish.py``, the module the
integration itself imports — runs against a live broker and a live near
end, and the client at the far end must receive the state of the
entities, opened.

The harness of the other surfaces is JavaScript, so this is the Python
equivalent of it: a real ``conduit-broker``, a real ``conduit-near-end``
with this surface's declaration, the plugin's own publishing code, and a
real client at the far end.

The far end is the Node client of the repository, run as a child process
(``far_end.mjs``), so that the crypto on this side of the tunnel is the
browser's WebCrypto and the crypto on the other side is the Rust near
end.

Home Assistant itself is not running here: what is exercised is the
publishing half, not the integration's Home Assistant plumbing.
"""

from __future__ import annotations

import asyncio
import json
import os
import queue
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent

# The check imports the plugin's modules and the shared client of the
# near end; the bytecode of those imports is not wanted in the tree.
sys.dont_write_bytecode = True

# The plugin's own code: the pure half, and the client of the near end.
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(ROOT / "clients" / "native"))

from near_end import NearEndClient  # noqa: E402
from publish import STATE_SCHEMA, entity_payload, publish_entities  # noqa: E402

#: The entities the user listed, and what Home Assistant knows of them.
ENTITIES: Dict[str, Dict[str, Any]] = {
    "light.kitchen": {
        "state": "on",
        "attributes": {"friendly_name": "Kitchen", "brightness": 254},
        "last_changed": "2026-01-01T09:00:00+00:00",
    },
    "sensor.living_room_temperature": {
        "state": "21.5",
        "attributes": {"unit_of_measurement": "C", "device_class": "temperature"},
        "last_changed": "2026-01-01T09:00:01+00:00",
    },
}


class Lines:
    """The lines a child process writes, read on a thread.

    A pipe has to be drained while the child is alive, and a line has to
    be waited for, so the reading is on its own thread.

    :param stream: the stream to read.
    """

    def __init__(self, stream: Any) -> None:
        self.queue: "queue.Queue[Optional[str]]" = queue.Queue()
        self.thread = threading.Thread(target=self._read, args=(stream,), daemon=True)
        self.thread.start()

    def _read(self, stream: Any) -> None:
        for line in stream:
            self.queue.put(line)
        self.queue.put(None)

    def next(self, timeout: float = 15.0) -> str:
        """The next line, or an error when there is none in time.

        :param timeout: how long to wait, in seconds.
        :returns: the line, without its newline.
        """
        try:
            line = self.queue.get(timeout=timeout)
        except queue.Empty:
            raise TimeoutError("the child process wrote nothing in time") from None
        if line is None:
            raise RuntimeError("the child process stopped writing")
        return line.rstrip("\n")


def binary(name: str) -> str:
    """The path of a built binary of the repository.

    :param name: the name of the binary.
    :returns: the path of the binary.
    """
    path = ROOT / "target" / "debug" / name
    if not path.exists():
        raise SystemExit(
            f"FAIL {path} is not there; run "
            "`cargo build -p conduit-broker -p conduit-near-end` first"
        )
    return str(path)


def start_broker(directory: Path, surface: Dict[str, Any]) -> Tuple[str, subprocess.Popen]:
    """Start the broker, with this surface in its bootstrap.

    :param directory: a directory of its own for the run.
    :param surface: the declaration of the surface.
    :returns: the address of the broker, and the process.
    """
    bootstrap = directory / "bootstrap.json"
    bootstrap.write_text(
        json.dumps(
            {
                "token": "bootstrap-token",
                "surfaces": [
                    {
                        "id": surface["id"],
                        "schemas": surface.get("schemas") or [],
                        "scopes": surface.get("scopes") or [],
                    }
                ],
                "grants": [
                    {
                        "surface": surface["id"],
                        "instance": "bootstrap",
                        "scopes": surface.get("scopes") or [],
                    }
                ],
            }
        ),
        encoding="utf-8",
    )
    broker = subprocess.Popen(
        [binary("conduit-broker")],
        env={
            **os.environ,
            "CONDUIT_LISTEN": "127.0.0.1:0",
            "CONDUIT_BOOTSTRAP": str(bootstrap),
            "CONDUIT_CLIENT_TOKEN": "bootstrap-token",
        },
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    out = Lines(broker.stdout)
    address = ""
    deadline = time.time() + 15
    while time.time() < deadline:
        line = out.next(timeout=15)
        if "listening on" in line:
            address = line.split("listening on ")[1].replace("/v1/tunnel", "").strip()
            break
    if not address:
        raise RuntimeError("the broker did not say where it was listening")
    return address, broker


def start_near_end(
    directory: Path, surface: Dict[str, Any], address: str
) -> Tuple[Path, subprocess.Popen]:
    """Start the near end, with this surface's declaration.

    :param directory: a directory of its own for the run.
    :param surface: the declaration of the surface.
    :param address: the address of the broker.
    :returns: the path of the socket of the near end, and the process.
    """
    socket = directory / "near-end.sock"
    configuration = directory / "near-end.json"
    configuration.write_text(
        json.dumps(
            {
                "broker": f"{address}/v1/tunnel",
                "instance": "bootstrap",
                "surfaces": [
                    {
                        "id": surface["id"],
                        "channel": surface.get("channel") or surface["id"],
                        "schemas": surface.get("schemas") or [],
                        "scopes": surface.get("scopes") or [],
                        "socket": str(socket),
                        "secret": "bootstrap-secret",
                    }
                ],
            }
        ),
        encoding="utf-8",
    )
    near_end = subprocess.Popen(
        [binary("conduit-near-end"), str(configuration)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    deadline = time.time() + 15
    while not socket.exists():
        if time.time() > deadline:
            raise RuntimeError("the near end did not bind its socket")
        time.sleep(0.05)
    return socket, near_end


def start_far_end(
    surface: Dict[str, Any], address: str, expected: int
) -> Tuple[subprocess.Popen, Lines]:
    """Start the client at the far end of the tunnel.

    :param surface: the declaration of the surface.
    :param address: the address of the broker.
    :param expected: how many events to wait for.
    :returns: the process, and the lines it writes.
    """
    far_end = subprocess.Popen(
        ["node", str(HERE / "far_end.mjs")],
        env={
            **os.environ,
            "CONDUIT_BROKER": f"{address}/v1/tunnel",
            "CONDUIT_SURFACE": surface["id"],
            "CONDUIT_CHANNEL": surface.get("channel") or surface["id"],
            "CONDUIT_TOKEN": "bootstrap-token",
            "CONDUIT_SCOPES": ",".join(surface.get("scopes") or []),
            "CONDUIT_EXPECT": str(expected),
        },
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    return far_end, Lines(far_end.stdout)


def check_the_files() -> None:
    """Check the files Home Assistant wants, and that they are Python.

    Home Assistant is not running here, so this is all that can be said
    about the integration's own files.
    """
    for name in ("__init__.py", "publish.py"):
        source = (HERE / name).read_text(encoding="utf-8")
        try:
            compile(source, str(HERE / name), "exec")
        except SyntaxError as error:
            raise AssertionError(f"{name} is not Python ({error})") from error
    manifest = json.loads((HERE / "manifest.json").read_text(encoding="utf-8"))
    for key in ("domain", "name", "version", "documentation", "requirements"):
        if key not in manifest:
            raise AssertionError(f"the manifest has no {key}")
    if manifest["domain"] != "conduit":
        raise AssertionError("the domain of the integration is not conduit")


def check_the_payload() -> None:
    """Check the payload of an entity, which is pure arithmetic."""
    payload = entity_payload(
        "light.kitchen",
        {"state": "on", "attributes": {"friendly_name": "Kitchen"}},
    )
    if payload["entity_id"] != "light.kitchen" or payload["state"] != "on":
        raise AssertionError(f"the payload was {payload}")
    if payload["attributes"].get("friendly_name") != "Kitchen":
        raise AssertionError(f"the attributes were {payload['attributes']}")
    # An entity that does not exist has no state.
    missing = entity_payload("light.nowhere", None)
    if missing["state"] != "" or missing["attributes"] != {}:
        raise AssertionError(f"the payload was {missing}")


async def run() -> None:
    """The check itself."""
    registry = json.loads(
        (HERE.parents[1] / "spec" / "surfaces.json").read_text(encoding="utf-8")
    )
    surface = next(
        entry for entry in registry["surfaces"] if entry["name"] == "home-assistant"
    )
    if STATE_SCHEMA not in (surface.get("schemas") or []):
        raise AssertionError(f"the surface does not declare {STATE_SCHEMA}")
    check_the_files()
    check_the_payload()

    with tempfile.TemporaryDirectory(prefix="conduit-home-assistant-") as name:
        directory = Path(name)
        address, broker = start_broker(directory, surface)
        near_end_process: Optional[subprocess.Popen] = None
        far_end: Optional[subprocess.Popen] = None
        try:
            socket, near_end_process = start_near_end(directory, surface, address)
            far_end, lines = start_far_end(surface, address, len(ENTITIES))

            # The far end is connected and the tunnel has agreed its keys.
            ready = json.loads(lines.next(timeout=15))
            if not ready.get("ready"):
                raise AssertionError(f"the far end said {ready}")

            # The plugin's own code: the pure half of the integration.
            client = await NearEndClient(str(socket)).connect()
            published = await publish_entities(client, ENTITIES)
            if published != len(ENTITIES):
                raise AssertionError(f"the integration published {published} entities")

            # What the client at the far end received.
            received: List[Dict[str, Any]] = []
            for _ in range(len(ENTITIES)):
                message = json.loads(lines.next(timeout=15))
                received.append(message["event"])

            for event in received:
                if event["schema"] != STATE_SCHEMA:
                    raise AssertionError(f"the schema was {event['schema']}")
                payload = event["payload"]
                if not isinstance(payload, dict):
                    raise AssertionError(f"the payload was {payload}")
                if payload["entity_id"] not in ENTITIES:
                    raise AssertionError(f"the payload was {payload}")
                expected = ENTITIES[payload["entity_id"]]
                if payload["state"] != expected["state"]:
                    raise AssertionError(f"the payload was {payload}")
                if event["sealed"] == json.dumps(payload):
                    raise AssertionError("the payload crossed the broker in the clear")
                if not isinstance(event["sealed"], str) or not event["sealed"]:
                    raise AssertionError("the payload was not sealed")

            await client.close()
        finally:
            for process in (far_end, near_end_process, broker):
                if process is not None:
                    process.kill()


def main() -> int:
    """Run the check, and say what happened.

    :returns: the exit code.
    """
    name = "the Home Assistant surface publishes the state of the entities"
    try:
        asyncio.run(run())
    except Exception as error:  # noqa: BLE001 - the check says what failed
        print(f"FAIL {name}: {error}")
        return 1
    print(f"ok   {name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
