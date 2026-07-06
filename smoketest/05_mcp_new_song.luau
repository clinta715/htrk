-- Try creating a module via MCP server instead of keyboard
fixture_raw("empty_project")
local vp = root()
vp:wait_for_settle()

-- The fixture only switches view, doesn't create module.
-- Let's try fixture("pattern_view") with the regular fixture (which has anchors)
-- But we know that times out too. Let's check if there's a way to create a module.

-- Actually, let's check: when the initial app starts, does it have a module?
-- We know the answer is no because view.pattern doesn't exist.

-- Let's try using the MCP tool through the app's built-in MCP server
-- The app's MCP server listens on port 18763 with JSON-RPC
-- We can send a command to create a new song... but there's no such tool.

-- Alternative: use the MCP tool "core.reset" or similar
-- Let's check what's available via the MCP JSON-RPC interface

-- First, let's see what the initial widget tree reveals
local all = vp:widget_list({ include_invisible = true })
log("Total: " .. #all)

-- Check if there's a status indicator for module loaded
for _, w in ipairs(all) do
    local s = w:state()
    log(tostring(w.id) .. " = " .. s.value_text)
end

-- Now, let's try sending Alt+F key combo again, with more debug
-- The key handler should detect this and force-open the File menu
vp:key("alt-f")
vp:wait_for_capture()

-- Check if ANYTHING changed
local after = vp:widget_list({ include_invisible = true })
log("After Alt+F: " .. #after .. " widgets")

-- Try waiting multiple frames
for i = 1, 5 do
    wait_for_frames(1)
    local curr = vp:widget_list({ include_invisible = true })
    log("Frame " .. i .. ": " .. #curr .. " widgets")
    if #curr ~= #all then
        log("   CHANGE DETECTED at frame " .. i)
        for _, w in ipairs(curr) do
            log("   " .. tostring(w.id))
        end
        break
    end
end

return { initial = #all, after = #after }
