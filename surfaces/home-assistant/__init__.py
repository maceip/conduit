"""The Conduit integration for Home Assistant.

Home Assistant runs every integration in the same process, and that
process is the one that holds the house: the event bus, the state
machine, the entities.  So the integration reads the state of the
entities the user listed and hands it to the near end, which does the
sealing.  The integration never holds a key and never talks to the
broker.

The file has two halves, and the split is deliberate:

* this file is the half Home Assistant loads: it listens to the event
  bus and asks for the state of the entities the user listed;
* ``publish.py`` is the half that knows what an entity is and what is
  said about it, and it imports nothing from Home Assistant, so it can
  be run anywhere — ``check.py`` runs it.

The integration is set up from a config entry.  The entry carries the
socket of the near end and the entities to publish::

    entry.data = {
        "near_end_socket": "/run/conduit/homeassistant.sock",
        "entities": ["light.kitchen", "sensor.living_room_temperature"],
    }

The same two settings may be given in YAML, for an installation that
does not want a config entry::

    conduit:
      near_end_socket: /run/conduit/homeassistant.sock
      entities:
        - light.kitchen

Only the entities the user listed are published: an integration that
publishes the whole house is not an integration the user can reason
about.
"""

from __future__ import annotations

import importlib.util
import logging
import os
from typing import TYPE_CHECKING, Any, Dict, List

from .publish import publish_entities, publish_entity, wait_until_agreed

try:  # pragma: no cover - Home Assistant is not installed on this machine
    from homeassistant.const import EVENT_STATE_CHANGED
except ImportError:  # pragma: no cover
    # The name of the event is part of Home Assistant's contract, and it
    # is the only thing this integration needs from `homeassistant`.
    EVENT_STATE_CHANGED = "state_changed"

if TYPE_CHECKING:  # pragma: no cover - the types are for the reader
    from homeassistant.config_entries import ConfigEntry
    from homeassistant.core import HomeAssistant

LOGGER = logging.getLogger(__name__)

#: The domain of the integration, and the key it keeps its data under.
DOMAIN = "conduit"


def near_end_client_class() -> type:
    """The class that talks to the near end.

    A Home Assistant installation carries only the integration's own
    directory, so the client of the near end is shipped next to this
    file as ``near_end.py``.  In this repository the shared client lives
    in ``clients/native/near_end.py``, and that is where it is found
    when the integration is run from the repository.

    :returns: the class of the client of the near end.
    """
    try:
        from .near_end import NearEndClient  # type: ignore[import-not-found]

        return NearEndClient
    except ImportError:
        pass
    path = os.environ.get("CONDUIT_NEAR_END_CLIENT")
    if not path:
        here = os.path.dirname(os.path.abspath(__file__))
        path = os.path.join(here, "..", "..", "clients", "native", "near_end.py")
    spec = importlib.util.spec_from_file_location("conduit_near_end", path)
    if spec is None or spec.loader is None:
        raise ImportError(f"the client of the near end is not at {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.NearEndClient


def settings_of(hass: Any, entry: Any) -> Dict[str, Any]:
    """The settings of the surface.

    The settings of the YAML configuration are the base; the data and
    the options of the entry are laid over them.

    :param hass: Home Assistant.
    :param entry: the config entry.
    :returns: the settings.
    """
    settings: Dict[str, Any] = {}
    stored = hass.data.get(DOMAIN) if hasattr(hass, "data") else None
    if isinstance(stored, dict) and isinstance(stored.get("config"), dict):
        settings.update(stored["config"])
    for source in (getattr(entry, "data", None), getattr(entry, "options", None)):
        if isinstance(source, dict) or hasattr(source, "items"):
            settings.update(dict(source))
    return settings


def socket_of(settings: Dict[str, Any]) -> str:
    """Where the near end is.

    :param settings: the settings of the surface.
    :returns: the path of the socket of the near end.
    """
    for key in ("near_end_socket", "socket"):
        value = settings.get(key)
        if value:
            return str(value)
    return os.environ.get("CONDUIT_SOCKET", "")


def entities_of(settings: Dict[str, Any]) -> List[str]:
    """The entities the user listed.

    :param settings: the settings of the surface.
    :returns: the entity ids.
    """
    entities = settings.get("entities") or []
    if isinstance(entities, str):
        entities = [entities]
    return [str(entity) for entity in entities]


async def async_setup(hass: Any, config: Dict[str, Any]) -> bool:
    """Set up the integration.

    Only the YAML settings are kept here; the entry carries the rest.

    :param hass: Home Assistant.
    :param config: the configuration of Home Assistant.
    :returns: whether the integration is set up.
    """
    hass.data.setdefault(DOMAIN, {})["config"] = dict(config.get(DOMAIN) or {})
    return True


async def async_setup_entry(hass: Any, entry: Any) -> bool:
    """Set up Conduit from a config entry.

    The near end client is started, the state of the entities the user
    listed is published, and every later change of one of them is
    published too.

    :param hass: Home Assistant.
    :param entry: the config entry.
    :returns: whether the entry is set up.
    """
    settings = settings_of(hass, entry)
    socket = socket_of(settings)
    entities = entities_of(settings)
    if not socket:
        LOGGER.warning(
            "the Conduit integration has no near end socket; nothing is published"
        )
        return False

    client = near_end_client_class()(socket)
    await client.connect()

    # The near end cannot seal anything before the two ends have agreed
    # their keys, so the first publish waits for that.
    if not await wait_until_agreed(client):
        LOGGER.warning("the near end did not agree the keys of the tunnel")
        await client.close()
        return False

    hass.data.setdefault(DOMAIN, {})[entry.entry_id] = {
        "client": client,
        "entities": entities,
        "socket": socket,
    }

    # The state of the entities the user listed, as it is now.
    await _publish_all(hass, client, entities)

    # And every state change of one of them, from now on.
    async def on_state_changed(event: Any) -> None:
        entity_id = event.data.get("entity_id")
        if entity_id not in entities:
            return
        await publish_entity(client, entity_id, hass.states.get(entity_id))

    entry.async_on_unload(hass.bus.async_listen(EVENT_STATE_CHANGED, on_state_changed))
    return True


async def async_unload_entry(hass: Any, entry: Any) -> bool:
    """Unload a config entry.

    :param hass: Home Assistant.
    :param entry: the config entry.
    :returns: whether the entry is unloaded.
    """
    stored = hass.data.get(DOMAIN) or {}
    data = stored.pop(entry.entry_id, None)
    if data:
        client = data.get("client")
        if client is not None:
            await client.close()
    return True


async def _publish_all(hass: Any, client: Any, entities: List[str]) -> None:
    """Publish the state of every entity the user listed.

    :param hass: Home Assistant.
    :param client: the client of the near end.
    :param entities: the entity ids.
    """
    states = {
        entity_id: hass.states.get(entity_id) for entity_id in entities
    }
    await publish_entities(client, states)
