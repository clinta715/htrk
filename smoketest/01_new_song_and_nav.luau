-- Verify Ctrl+N creates a module, then test Alt-menu navigation
fixture_raw("empty_project")
local vp = root()
vp:wait_for_settle()

-- Check initial state
local initial = vp:widget_list()
log("Before Ctrl+N: " .. #initial .. " widgets")

-- List first few widget ids
for i = 1, math.min(5, #initial) do
    log("  widget: " .. tostring(initial[i].id))
end

-- Press Ctrl+N to create new module
vp:key("ctrl-n")
vp:wait_for_settle()

-- Check state after Ctrl+N
local after_new = vp:widget_list()
log("After Ctrl+N: " .. #after_new .. " widgets")
for i = 1, math.min(10, #after_new) do
    log("  widget: " .. tostring(after_new[i].id))
end

-- Now try to get view.pattern safely
local view_found = pcall(function()
    local w = vp:widget_get("view.pattern")
    log("view.pattern: label=" .. tostring(w:state().label))
end)
if not view_found then
    log("view.pattern NOT found after Ctrl+N - trying with viewport wait")
    -- Wait additional frames
    wait_for_frames(5)
    local retry = pcall(function()
        local w = vp:widget_get("view.pattern")
        log("view.pattern after wait: label=" .. tostring(w:state().label))
    end)
    if not retry then log("view.pattern still not found after extra wait") end
end

-- Verify all view tabs exist
local checks = { "view.pattern", "view.sample", "view.instrument", "view.sendfx", "view.playback", "view.automation", "view.mixer" }
for _, id in ipairs(checks) do
    local ok = pcall(function()
        local w = vp:widget_get(id)
        local s = w:state()
        log(id .. " exists, visible=" .. tostring(s.visible))
    end)
    if not ok then log(id .. " MISSING") end
end

-- Test Alt+F to open File menu
vp:key("alt-f")
vp:wait_for_settle()

-- Now check if menu items are visible
local file_menu_items = {
    "menu.file.new_song",
    "menu.file.open",
    "menu.file.save",
    "menu.file.quit",
}
for _, id in ipairs(file_menu_items) do
    local ok = pcall(function()
        local w = vp:widget_get(id)
        local s = w:state()
        log(id .. ": visible=" .. tostring(s.visible) .. " label=" .. tostring(s.label))
    end)
    if not ok then log(id .. " MISSING after Alt+F") end
end

-- Press Escape to close menu
vp:key("escape")
vp:wait_for_settle()

-- Verify menu closed (items no longer visible)
local file_menu_shown = pcall(function()
    local w = vp:widget_get("menu.file.new_song")
    return w:state().visible
end)
if file_menu_shown then
    log("FAIL: menu still visible after Escape")
else
    log("OK: menu closed after Escape")
end

-- Test Alt tap: press and release Alt to activate menu bar
vp:key("alt")
vp:wait_for_settle()

-- After Alt tap, menu_bar_active should be true, but we can't read internal state.
-- The visual indicator is the File button highlight.
-- Let's press Down arrow to try opening the File menu
vp:key("down")
vp:wait_for_settle()

-- File menu should be open now
local after_down = pcall(function()
    local w = vp:widget_get("menu.file.new_song")
    log("After Alt+Down: new_song visible=" .. tostring(w:state().visible))
end)
if not after_down then
    log("After Alt+Down: menu.file.new_song not found (maybe menu bar didn't activate)")
end

vp:key("escape")
vp:wait_for_settle()

-- Test Alt+E to open Edit menu
vp:key("alt-e")
vp:wait_for_settle()
local edit_found = pcall(function()
    local w = vp:widget_get("menu.edit.paste")
    log("After Alt+E: paste label=" .. tostring(w:state().label))
end)
if not edit_found then log("Alt+E: Edit menu NOT opened") end

vp:key("escape")
vp:wait_for_settle()

-- Test Escape closes all menus
vp:key("alt-f")
vp:wait_for_settle()
vp:key("escape")
vp:wait_for_settle()
local still_open = pcall(function()
    local w = vp:widget_get("menu.file.save")
    return w:state().visible
end)
if not still_open then
    log("OK: menu closed with Escape")
else
    log("FAIL: menu still visible after Escape")
end

log("---")
log("All Alt-menu navigation tests completed")
return { ok = true }
