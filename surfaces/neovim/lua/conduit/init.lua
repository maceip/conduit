-- The Conduit surface for Neovim.
--
-- Neovim runs every file in plugin/ when it starts, so a plugin is just a
-- file in the right place: there is no manifest and no registration step.
-- plugin/conduit.lua is the file Neovim runs; this module is what it uses.
--
-- The plugin publishes what the editor knows in plain JSON to the near
-- end, which holds the socket to the broker and does the sealing.  The
-- plugin never holds a key and never seals anything itself.
--
-- The protocol is one JSON object per message: a four-byte big-endian
-- length, and then that many bytes of JSON.
--
--     plugin  ->  {"t":"publish","s":"dev.conduit.neovim.buffer/v1","c":"buffer","p":{...}}
--     near end ->  {"t":"publish","s":"...","c":"...","seq":12,"p":{...}}

local M = {}

--- The schema of a buffer.
M.BUFFER_SCHEMA = "dev.conduit.neovim.buffer/v1"

--- Where the near end is, when nobody says otherwise.
M.DEFAULT_SOCKET = "/tmp/conduit-neovim.sock"

--- The channel this plugin publishes on.
M.CHANNEL = "buffer"

--- How long between two publishes of the same buffer, in milliseconds.
M.THROTTLE = 1000

local uv = vim.uv or vim.loop

--- The pipe to the near end, or nil when there is not one yet.
local pipe = nil

--- Whether a connection is being made.
local connecting = false

--- The bytes that have come in but do not make a message yet.
local incoming = ""

--- Whether the tunnel is up.  The near end says so with the welcome
--- frame.  Nothing waits for it: the near end holds what is published
--- until there is a key to seal it under, and sends it then.
local up = false

--- When the buffer was published last, in milliseconds.
local last_publish = 0

--- What happens when the near end says something.
local handlers = {}

--- Say something about the plugin.
local function complain(message)
  vim.schedule(function()
    vim.notify("conduit: " .. message, vim.log.levels.WARN)
  end)
end

--- The JSON of a value.
local function encode(value)
  if vim.json ~= nil and vim.json.encode ~= nil then
    return vim.json.encode(value)
  end
  return vim.fn.json_encode(value)
end

--- The value of some JSON.
local function decode(text)
  if vim.json ~= nil and vim.json.decode ~= nil then
    return vim.json.decode(text)
  end
  return vim.fn.json_decode(text)
end

--- A message: a four-byte big-endian length, and then the JSON.
local function frame(message)
  local body = encode(message)
  local length = #body
  local header = string.char(
    math.floor(length / 16777216) % 256,
    math.floor(length / 65536) % 256,
    math.floor(length / 256) % 256,
    length % 256)
  return header .. body
end

--- Write a message to the near end.
local function send(message)
  if pipe == nil then
    return false
  end
  local ok, err = pcall(function()
    pipe:write(frame(message))
  end)
  if not ok then
    complain("the message could not be written: " .. tostring(err))
    return false
  end
  return true
end

--- Take the messages out of what has come in.
local function take_messages()
  local messages = {}
  while #incoming >= 4 do
    local b1, b2, b3, b4 = string.byte(incoming, 1, 4)
    local length = b1 * 16777216 + b2 * 65536 + b3 * 256 + b4
    if #incoming < 4 + length then
      break
    end
    local body = string.sub(incoming, 5, 4 + length)
    incoming = string.sub(incoming, 5 + length)
    local ok, message = pcall(decode, body)
    if ok and type(message) == "table" then
      table.insert(messages, message)
    end
  end
  return messages
end


--- Remember what the near end said.
local function note(message)
  if message.t == "welcome" then
    up = true
  elseif message.t == "error" then
    complain("the near end refused: " .. tostring(message.p and message.p.message))
  end
  for _, handler in ipairs(handlers) do
    vim.schedule(function()
      handler(message)
    end)
  end
end

--- Open the socket to the near end.
local function connect()
  if pipe ~= nil or connecting then
    return
  end
  connecting = true
  local handle = uv.new_pipe(false)
  handle:connect(M.socket_path(), function(err)
    connecting = false
    if err ~= nil then
      vim.schedule(function()
        complain("the near end is not there: " .. tostring(err))
      end)
      handle:close()
      return
    end
    pipe = handle
    handle:read_start(function(read_err, chunk)
      if read_err ~= nil then
        vim.schedule(function()
          complain("the socket to the near end broke: " .. tostring(read_err))
        end)
        handle:close()
        pipe = nil
        return
      end
      if chunk ~= nil then
        incoming = incoming .. chunk
        for _, message in ipairs(take_messages()) do
          note(message)
        end
      end
    end)
    start_asking()
  end)
end

--- Where the near end is.
---
--- The option of `setup` first, then the environment, and then the
--- default.
---
--- @return string
function M.socket_path()
  if M.socket_path_override ~= nil and M.socket_path_override ~= "" then
    return M.socket_path_override
  end
  local path = vim.env.CONDUIT_SOCKET
  if path ~= nil and path ~= "" then
    return path
  end
  return M.DEFAULT_SOCKET
end

--- Publish something, once the tunnel has a key.
---
--- The near end cannot seal anything until the two ends have agreed their
--- keys, and a publish before that is refused.  So the publish waits for
--- the tunnel to agree, with a timeout: until then the publish waits in
--- the queue, and if the tunnel does not agree in time it is dropped.
---
--- @param schema string the schema of the payload
--- @param payload table the payload
--- @return boolean whether the publish was sent
function M.publish(schema, payload)
  connect()
  return send({ t = "publish", s = schema, c = M.CHANNEL, seq = 0, p = payload })
end

--- What is said about the buffer in front of the user.
---
--- @return table the payload
function M.buffer_payload()
  local buffer = vim.api.nvim_get_current_buf()
  local name = vim.api.nvim_buf_get_name(buffer)
  local cursor = vim.api.nvim_win_get_cursor(0)
  return {
    path = name,
    name = vim.fn.fnamemodify(name, ":t"),
    lines = vim.api.nvim_buf_line_count(buffer),
    cursorLine = cursor[1],
    cursorColumn = cursor[2],
    modified = vim.bo[buffer].modified,
    filetype = vim.bo[buffer].filetype,
  }
end

--- Publish the buffer in front of the user.
---
--- The buffer is published when it changes, and the changes come faster
--- than a tunnel wants them, so the publishes are throttled.
---
--- @param force boolean publish even inside the throttle
--- @return boolean whether the publish was sent
function M.publish_buffer(force)
  local now = uv.now()
  if not force and now - last_publish < M.THROTTLE then
    return false
  end
  last_publish = now
  return M.publish(M.BUFFER_SCHEMA, M.buffer_payload())
end

--- Say what happens when the near end says something.
---
--- @param handler function the handler
function M.on(handler)
  table.insert(handlers, handler)
end

--- Set the surface up.
---
--- @param options table options: `socket` and `throttle`
function M.setup(options)
  options = options or {}

  if options.socket ~= nil and options.socket ~= "" then
    M.socket_path_override = options.socket
  end
  if options.throttle ~= nil then
    M.THROTTLE = options.throttle
  end

  local group = vim.api.nvim_create_augroup("Conduit", { clear = true })
  vim.api.nvim_create_autocmd(
    { "BufEnter", "BufWritePost", "CursorMoved", "TextChanged" },
    {
      group = group,
      callback = function()
        M.publish_buffer()
      end,
    })

  vim.api.nvim_create_user_command("ConduitPublish", function()
    M.publish_buffer(true)
  end, { desc = "Publish the current buffer to a Conduit client." })

  vim.schedule(function()
    M.publish_buffer(true)
  end)
end

return M
