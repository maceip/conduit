-- The file Neovim loads.
--
-- Neovim runs every file in plugin/ when it starts, so there is no
-- manifest and no registration step: this file is the plugin.  It asks for
-- the module and sets it up; the module is what knows what a buffer is and
-- how to say it.

if vim.g.loaded_conduit == 1 then
  return
end
vim.g.loaded_conduit = 1

require("conduit").setup()
