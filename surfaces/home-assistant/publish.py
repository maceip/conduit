"""What the Home Assistant surface publishes.

The integration in ``__init__.py`` is the half that Home Assistant loads:
it listens to the event bus and asks for the state of the entities the
user listed.  This module is the other half, and it is the half that knows
what an entity is and what is said about it.

The split is deliberate: **nothing here imports Home Assistant**.  The
state of an entity is read with ``hass.states.get(entity_id)`` and handed
over as a plain mapping (or as the state object itself, which is read the
same way), so the code that decides *what* is published can be run — and
is run by ``check.py`` — anywhere.

The publishing itself is plain JSON on the socket of the near end.  The
near end seals it; this module never holds a key.
"""

from __future__ import annotations

import asyncio
from typing import Any, Iterable, Mapping, Tuple

#: The schema of the state of an entity.
STATE_SCHEMA = "dev.conduit.homeassistant.state/v1"


def entity_payload(entity_id: str, state: Any) -> dict[str, Any]:
    """What is said about an entity.

    The state is whatever Home Assistant knows about the entity: either a
    mapping with the keys ``state``, ``attributes`` and ``last_changed``,
    or an object with those attributes — which is what
    ``hass.states.get(entity_id)`` returns.  An entity that does not
    exist has no state, and is said to be in the state ``""``.

    :param entity_id: the entity, for example ``light.kitchen``.
    :param state: the state of that entity.
    :returns: the payload, as JSON.
    """
    if isinstance(state, Mapping):
        value = state.get("state", "")
        attributes = state.get("attributes") or {}
        last_changed = state.get("last_changed", "")
    else:
        value = getattr(state, "state", "")
        attributes = getattr(state, "attributes", None) or {}
        last_changed = getattr(state, "last_changed", "")
    return {
        "entity_id": entity_id,
        "state": "" if value is None else str(value),
        "attributes": {str(key): item for key, item in dict(attributes).items()},
        "last_changed": "" if last_changed is None else str(last_changed),
    }


def entity_pairs(entities: Any) -> Iterable[Tuple[str, Any]]:
    """The entities, as pairs of an entity id and its state.

    The entities are either a mapping of an entity id to the state of
    that entity, or an iterable of such pairs.

    :param entities: the entities.
    :returns: the pairs.
    """
    if isinstance(entities, Mapping):
        return list(entities.items())
    return [(entity_id, state) for entity_id, state in entities]


async def wait_until_agreed(client: Any, timeout: float = 10.0) -> bool:
    """Wait until the two ends of the tunnel have agreed their keys.

    The near end cannot seal anything before it has met the other end,
    and a publish that arrives before then is refused.  So the plugin
    asks how the tunnel is until the answer says the tunnel is agreed.

    :param client: the client of the near end.
    :param timeout: how long to wait, in seconds.
    :returns: whether the tunnel agreed.
    """
    agreed = asyncio.Event()

    def watch(message: Mapping[str, Any]) -> None:
        if message.get("op") == "status" and message.get("agreed"):
            agreed.set()

    client.on(watch)
    loop = asyncio.get_running_loop()
    deadline = loop.time() + timeout
    while not agreed.is_set():
        remaining = deadline - loop.time()
        if remaining <= 0:
            return False
        await client.status()
        try:
            await asyncio.wait_for(agreed.wait(), timeout=min(0.2, remaining))
        except asyncio.TimeoutError:
            continue
    return True


async def publish_entities(client: Any, entities: Any) -> int:
    """Publish the state of every entity.

    The state of each entity is published to the near end, in plain JSON,
    under the schema ``dev.conduit.homeassistant.state/v1``.  The near end
    seals it; this function never sees a key.

    :param client: the client of the near end.
    :param entities: the entities, as a mapping of an entity id to the
        state of that entity, or as an iterable of such pairs.
    :returns: how many entities were published.
    """
    if not await wait_until_agreed(client):
        raise TimeoutError("the near end did not agree the keys of the tunnel")
    published = 0
    for entity_id, state in entity_pairs(entities):
        await client.publish(STATE_SCHEMA, entity_payload(entity_id, state))
        published += 1
    return published


async def publish_entity(client: Any, entity_id: str, state: Any) -> None:
    """Publish the state of one entity.

    :param client: the client of the near end.
    :param entity_id: the entity, for example ``light.kitchen``.
    :param state: the state of that entity.
    """
    await client.publish(STATE_SCHEMA, entity_payload(entity_id, state))
