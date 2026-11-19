# The Neovim surface

A Neovim plugin is any Vimscript or Lua file that lives in the right
directory: there is no manifest and no registration step.  The runtime
runs every file in `plugin/**/*.{vim,lua}` when it starts, and the modules
a plugin ships live under `lua/<name>/`.  A whole package — a directory
with both of those in it — is loaded when it sits in
`pack/*/start/<name>/`.

This surface is one such package:

| the part | the file |
|---|---|
| the declaration | `surface.json` |
| the file Neovim loads | `plugin/conduit.lua` |
| the module | `lua/conduit/init.lua` |

## What it publishes

The buffer in front of the user: its path, its name, how many lines it
has, and where the cursor is, under the schema
`dev.conduit.neovim.buffer/v1`.

The plugin publishes in plain JSON to the near end.  It holds no key, it
opens no socket to the broker, and it seals nothing: the near end does the
sealing, and the broker only ever sees the sealed form.

Before the first publish it waits for the tunnel to agree its keys,
because the near end cannot seal anything until the two ends have met.
It asks `{"op":"status"}` and waits for `{"op":"status",…,"agreed":true}`,
for up to ten seconds; a publish that is sent before that is refused with
`{"op":"error"}`.  Until the tunnel agrees, the publish waits in a queue
and is sent the moment the near end says the tunnel is ready.

The buffer is published when the user moves in it, writes it or changes
it, and the changes are throttled to one publish a second, because a
tunnel does not want a keystroke each.

## What the user has to do

1. **Put the package where Neovim looks for packages.**  Either as a
   directory in the runtime path:

   ```
   ~/.local/share/nvim/site/pack/conduit/start/conduit/plugin/conduit.lua
   ~/.local/share/nvim/site/pack/conduit/start/conduit/lua/conduit/init.lua
   ```

   or anywhere at all, with the directory added to the runtime path in
   `init.lua`:

   ```lua
   vim.opt.runtimepath:append("/path/to/neovim")
   require("conduit").setup()
   ```

   A plugin manager (lazy.nvim, packer.nvim, …) does the same thing.
2. **Point it at the near end.**  The plugin looks for the socket in
   `CONDUIT_SOCKET`, and then at `/tmp/conduit-neovim.sock`.  The near end
   must be running with the same path.  The path can also be given in the
   setup:

   ```lua
   require("conduit").setup({ socket = "/tmp/conduit-neovim.sock" })
   ```
3. **Use it.**  The buffer is published as soon as Neovim starts and as
   the user works.  `:ConduitPublish` publishes it right away, without
   waiting for the throttle.

## What was verified

**Neovim is not installed on this machine, so the plugin was never
loaded.**  `plugin/conduit.lua` and `lua/conduit/init.lua` have never been
parsed by Lua, never been put on a runtime path, and never been run by an
editor.  There is no Neovim, no Lua interpreter and no `luac` on this
machine either, so even a syntax check of the two files could not be
made.  What was written was written by hand against Neovim's documented
Lua API (`vim.uv`/`vim.loop`, `nvim_create_autocmd`,
`nvim_buf_line_count`, …) and the plugin layout in Neovim's own
documentation, and the protocol against the contract in
`docs/surface-contract.md` and the Node client in
`clients/native/near-end.js`.

To run it, on a machine with Neovim:

* `luac -p plugin/conduit.lua && luac -p lua/conduit/init.lua` to parse
  the two files (or `nvim --headless -c 'luafile …'`),
* put the package on the runtime path and start Neovim with a near end
  running,
* and write a `check.mjs` in this directory that drives the *same*
  publishing code against a live broker and a live near end, the way
  `surfaces/vscode/check.mjs` does.  There is no such check here: it
  would have to run the Lua, and Neovim cannot run here.
