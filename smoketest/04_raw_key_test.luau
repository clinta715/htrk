-- Test using raw_key to inject events
fixture_raw("empty_project")
local vp = root()
vp:wait_for_settle()

-- Try raw_key: press ctrl, press n, release n, release ctrl
vp:raw_key("control_left", "press", { ctrl = true })
vp:wait_for_capture()
vp:raw_key("n", "press", { ctrl = true })
vp:raw_key("n", "release", { ctrl = true })
vp:raw_key("control_left", "release", { ctrl = false })
vp:wait_for_capture()

-- Check for any new widgets
local all = vp:widget_list({ include_invisible = true })
log("After raw Ctrl+N: " .. #all .. " widgets")

local view_count = 0
for _, w in ipairs(all) do
    local id_str = tostring(w.id)
    if string.find(id_str, "view") then
        view_count = view_count + 1
        log("  view widget: " .. id_str)
    end
end
log("View widgets: " .. view_count)

-- Also try clicking the "New Song" button through the menu
-- First try opening the File menu with Alt+F
vp:raw_key("alt_left", "press", { alt = true })
vp:wait_for_capture()
vp:raw_key("f", "press", { alt = true })
vp:raw_key("f", "release", { alt = true })
vp:raw_key("alt_left", "release", { alt = false })
vp:wait_for_capture()

-- Check after Alt+F
local after_f = vp:widget_list({ include_invisible = true })
log("After raw Alt+F: " .. #after_f .. " widgets")
for _, w in ipairs(after_f) do
    log("  " .. tostring(w.id))
end

return { count = #all, after_menu = #after_f }
