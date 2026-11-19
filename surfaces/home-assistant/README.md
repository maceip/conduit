# The Home Assistant surface

Home Assistant holds the house: the event bus, the state machine, the
entities.  This surface is a **custom integration** that publishes the
state of the entities the user listed to a Conduit client.

The integration never holds a key and never talks to the broker.  It
publishes plain JSON to the near end, which does the sealing.

| the part | the file |
|---|---|
| the declaration | `spec/surfaces.json` |
| the manifest Home Assistant loads | `manifest.json` |
| the integration | `__init__.py` |
| what an entity is | `publish.py` |
| the check | `check.py` |
| the client at the far end of the check | `far_end.mjs` |

## Installing it

The directory is the integration: copy it to
`config/custom_components/conduit/` in the Home Assistant configuration
directory, and the domain of the integration is `conduit`.  A custom
integration needs a `manifest.json` with at least a `domain` and a
`name`; it also needs a `version`, and `documentation` and
`requirements`, and that is what `manifest.json` has.

The integration is set up from a config entry, which carries the socket
of the near end and the entities to publish:

```python
entry.data = {
    "near_end_socket": "/run/conduit/homeassistant.sock",
    "entities": ["light.kitchen", "sensor.living_room_temperature"],
}
```

The same two settings may be given in YAML, for an installation that
does not want a config entry:

```yaml
conduit:
  near_end_socket: /run/conduit/homeassistant.sock
  entities:
    - light.kitchen
```

Only the entities the user listed are published.  An integration that
publishes the whole house is not an integration the user can reason
about.

## What it publishes

The state of each listed entity — its entity id, its state, its
attributes and when it last changed — under the schema
`dev.conduit.homeassistant.state/v1`.  The state is published once when
the entry is set up, and again whenever one of the listed entities
changes, which the integration hears on the Home Assistant event bus
(`state_changed`).

The client of the near end is `clients/native/near_end.py` in this
repository.  A Home Assistant installation carries only the integration's
own directory, so that file is shipped next to `__init__.py` as
`near_end.py` when the integration is installed; `__init__.py` looks for
it there first.

## What was verified

**Verified on this machine.** `check.py` starts a real broker and a
real near end with this surface's declaration, runs `publish.py` — the
same module `__init__.py` imports — with the client of the near end, and
the client at the far end receives the state of both entities, opened.
The check also compiles both Python files and checks that the manifest
has the keys Home Assistant wants.

**Not run.** Home Assistant itself was not running here, so only the
publishing half was exercised.  `__init__.py` has never been loaded by
Home Assistant: the event bus, `hass.states.get`, the config entry and
the unload path have not been run by the real thing, and the manifest
has not been checked by Home Assistant's own validator.  No config flow
is shipped, so no entry can be created from the UI either.
