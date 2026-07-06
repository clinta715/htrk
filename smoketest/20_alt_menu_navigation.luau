-- Verify Alt-menu keyboard navigation
-- Uses built-in "empty_project" fixture (registered via DevMcp::on_fixture)
fixture("empty_project")
local vp = root()

local function menu_item_visible(id)
    local ok, w = pcall(function() return vp:widget_get(id) end)
    if ok then return w:state().visible end
    return false
end

-- Test 1: Alt+F opens File menu
log("=== Test 1: Alt+F opens File menu ===")
vp:key("alt-f")
wait_for_frames(3)
assert(menu_item_visible("menu.file.new_song"), "Alt+F should open File menu")
assert(menu_item_visible("menu.file.open"), "menu.file.open visible")
assert(menu_item_visible("menu.file.quit"), "menu.file.quit visible")

-- Test 2: Escape closes File menu
log("=== Test 2: Escape closes File menu ===")
vp:key("escape")
wait_for_frames(3)
assert(not menu_item_visible("menu.file.new_song"), "Escape should close File menu")

-- Test 3: Alt+E opens Edit menu
log("=== Test 3: Alt+E opens Edit menu ===")
vp:key("alt-e")
wait_for_frames(3)
assert(menu_item_visible("menu.edit.paste"), "Alt+E should open Edit menu")
vp:key("escape")
wait_for_frames(2)

-- Test 4: Alt+A opens Audio menu
log("=== Test 4: Alt+A opens Audio menu ===")
vp:key("alt-a")
wait_for_frames(3)
assert(menu_item_visible("menu.audio.rate"), "Alt+A should open Audio menu")
vp:key("escape")
wait_for_frames(2)

-- Test 5: Alt+V opens View menu
log("=== Test 5: Alt+V opens View menu ===")
vp:key("alt-v")
wait_for_frames(3)
assert(menu_item_visible("menu.view.follow_playback"), "Alt+V should open View menu")
vp:key("escape")
wait_for_frames(2)

-- Test 6: Alt+H opens Help menu
log("=== Test 6: Alt+H opens Help menu ===")
vp:key("alt-h")
wait_for_frames(3)
assert(menu_item_visible("menu.help.about"), "Alt+H should open Help menu")
vp:key("escape")
wait_for_frames(2)

-- Test 7: Switching menus (Alt+F then Alt+E)
log("=== Test 7: Alt+F then Alt+E (switch menus) ===")
vp:key("alt-f")
wait_for_frames(2)
assert(menu_item_visible("menu.file.new_song"), "File menu open")
vp:key("alt-e")
wait_for_frames(3)
assert(menu_item_visible("menu.edit.paste"), "Edit menu open after Alt+E")
assert(not menu_item_visible("menu.file.new_song"), "File menu closed after Alt+E")
vp:key("escape")
wait_for_frames(2)

-- Test 8: Escape from File menu
log("=== Test 8: Escape from any menu ===")
vp:key("alt-f")
wait_for_frames(2)
vp:key("escape")
wait_for_frames(3)
assert(not menu_item_visible("menu.file.new_song"), "Escape from File menu works")

log("---")
log("All Alt-menu navigation tests PASSED")
return { pass = true }
