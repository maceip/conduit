# The contract of a surface

A **surface** is a place a Conduit client can talk to.  Every surface has
the same three parts, and the parts are the same whatever the surface
is: a browser tab, a Mail message, the state of a house.

| the part | what it is |
|---|---|
| **the app** | Mail, VS Code, Slack, Home Assistant.  The thing the user has open. |
| **the plugin** | The code inside the app that reads what the app knows. |
| **the near end** | The process that holds the socket to the broker and does the sealing. |

The plugin never sees a key and never holds a socket.  It publishes what
the app knows, in plain JSON, to the near end; the near end seals it and
sends it to the broker; the broker forwards it without opening it; the
client at the far end opens it with the key only the two ends share.

```
   the app  ──►  the plugin  ──►  the near end  ──►  the broker  ──►  the client
                (plain JSON)      (seals it)       (forwards it)   (opens it)
```

## The layout of a surface

Every surface lives in `surfaces/<name>/`:

| the file | what it is |
|---|---|
| the plugin | the code the app loads, in the language the app speaks |
| `check.mjs` or `check.py` | the test that drives the plugin against a real broker |
| `README.md` | what the surface is, what it publishes, and **what was verified** |

**There is one declaration of every surface, and it is in
`spec/surfaces.json`.**  The near end is told about a surface from
there, the checks read it, and the pages of the site are written from
it, so a surface cannot drift from what it says it is.  This is the
entry for the VS Code surface:

```json
{
  "name": "vscode",
  "id": "dev.conduit.vscode.tabs/v1",
  "channel": "tabs",
  "schemas": ["dev.conduit.vscode.tab/v1"],
  "scopes": ["vscode.tabs.read"],
  "socket": "/tmp/conduit-vscode.sock"
}
```

- `name` — the directory of the surface, and how the checks find it.
- `id` — the identity of the surface, reverse DNS with a version.
- `channel` — the channel of the surface; the identity, unless it says otherwise.
- `schemas` — the schemas the surface publishes.
- `scopes` — the scopes the surface needs in its grant.
- `socket` — the Unix socket the plugin talks to.
- `tcp` — the port on the loopback, for a plugin that cannot open a Unix socket.

The near end's own configuration adds two things that are not the
surface's business: the broker, and the secret of the grant.

```json
{
  "broker": "ws://127.0.0.1:8787/v1/tunnel",
  "instance": "bootstrap",
  "surfaces": [
    {
      "id": "dev.conduit.vscode.tabs/v1",
      "channel": "tabs",
      "socket": "/tmp/conduit-vscode.sock",
      "secret": "bootstrap-secret"
    }
  ]
}
```

## The protocol between the plugin and the near end

**There is no second vocabulary.**  The hop between a plugin and the
near end carries the frames of the tunnel, and the only thing that
differs is that the payload of a published frame is plaintext, because
the near end is the one that seals it.

One JSON object per message.  On a Unix socket each message is a
four-byte big-endian length and then that many bytes; on a web socket
the web socket frames the message itself.

**The plugin says:**

```json
{"t": "publish", "s": "dev.conduit.vscode.tab/v1", "c": "tabs", "seq": 0, "p": {"url": "https://example.test/"}}
{"t": "connect", "s": "", "c": "tabs", "seq": 0, "p": {"broker": "ws://127.0.0.1:8787/v1/tunnel", "secret": "…", "instance": "bootstrap"}}
```

**The near end says:**

```json
{"t": "publish", "s": "dev.conduit.vscode.tab/v1", "c": "tabs", "seq": 7, "p": {"url": "https://example.test/"}}
{"t": "welcome", "s": "dev.conduit.welcome/v1", "c": "tunnel", "seq": 0, "p": {"seq": 0, "surface": "dev.conduit.vscode.tabs/v1"}}
{"t": "error", "s": "dev.conduit.error/v1", "c": "tabs", "seq": 0, "p": {"code": "schema_unknown", "message": "that is not a schema id: …"}}
```

The `connect` frame is the one frame the hop has and the tunnel does
not: it says where the broker is and what secret may be presented on
it, for a plugin that did the device flow and handed the result to the
process that holds the socket.  **The broker refuses a `connect` frame**
— it belongs to the hop, not to the tunnel.

**A plugin does not wait for the tunnel to be up.**  It publishes when
it has something to say, and the near end holds what it published until
the two ends have agreed their keys, and sends it then.  The waiting
happens once, in the near end, and not in every plugin in every
language.  The near end says the tunnel is up with the `welcome` frame.

The clients are written already:

- Node: `clients/native/near-end.js` — `connect(socketPath)`.
- A web page: `clients/shared/near-end-web.js` — `connect(url)`.
- Python: `clients/native/near_end.py` — `await NearEndClient(path).connect()`.

## What a surface has to do to count as working here

A surface is **verified** when a test on this machine has:

1. started a real broker,
2. started a real near end with that surface's declaration,
3. run the plugin's **own** code — not a copy of it — against the near end,
4. and seen the client at the far end receive the payload, opened.

A surface that is only a file, or a file that was never run, is **not**
verified.  Say so in its `README.md`, in those words.

Some surfaces cannot be run on a machine that is not their own: a Mail
extension needs macOS and Xcode, a WordPress plugin needs PHP, a Neovim
plugin needs Neovim.  For those the code is written and **not run**,
and the `README.md` says which is which.

## The rules

- The plugin publishes **plaintext** to the near end.  The near end
  seals it.  A plugin that seals anything itself is doing it wrong.
- The plugin publishes a **frame of the tunnel**, not a message of its
  own.  A plugin that invents a vocabulary is doing it wrong.
- The plugin holds no key and opens no socket to the broker.
- The plugin does not know the broker's address, the grant or the
  secret: it knows one socket path, or one address on the loopback.
- The schema of a publish is the schema **as written**: `dev.conduit.mail.message/v1`
  is the first version of the schema `dev.conduit.mail.message`.
