-- P1 input-routing acceptance: dialog gates editing keys, Alt-menu, Escape.
-- Exercises the single-pass router (src/actions/input.rs) end to end:
--  1. F10 opens Settings (dialog). Escape closes it, edit mode unchanged.
--  2. Escape with no dialog toggles EDT->VIEW->EDT (registry fallback).
--  3. Alt+F is blocked while a dialog is open, works after close.
--  4. Arrow keys never disturb edit mode (cursor stays in pattern editor).
fixture("empty_project")
local vp = root()

local function get_widget_text(id)
    local ok, w = pcall(function() return vp:widget_get(id) end)
    if ok then return w:state().value_text or "" end
    return nil
end

local function menu_visible(id)
    local ok, w = pcall(function() return vp:widget_get(id) end)
    if ok then return w:state().visible end
    return false
end

assert(get_widget_text("status.mode") == "EDT", "starts in edit mode")

-- 1. Open Settings via F10, then Escape must close the dialog (not toggle mode)
log("--- F10 opens dialog; Escape closes it ---")
vp:key("f10")
wait_for_frames(5)
assert(get_widget_text("status.mode") == "EDT", "still EDT with dialog open")
vp:key("down")
vp:key("right")
vp:key("tab")
wait_for_frames(3)
assert(get_widget_text("status.mode") == "EDT", "nav keys blocked by dialog")
vp:key("escape")
wait_for_frames(5)
assert(get_widget_text("status.mode") == "EDT", "Escape closed dialog, mode kept")
log("PASS")

-- 2. Escape with no dialog toggles edit mode
log("--- Escape toggles EDT/VIEW with no dialog ---")
vp:key("escape")
wait_for_frames(3)
assert(get_widget_text("status.mode") == "VIEW", "Escape toggles to VIEW")
vp:key("escape")
wait_for_frames(3)
assert(get_widget_text("status.mode") == "EDT", "Escape toggles back to EDT")
log("PASS")

-- 3. Alt+F blocked while dialog open, works after close
log("--- Alt+F gated on dialog ---")
vp:key("f10")
wait_for_frames(5)
vp:key("alt-f")
wait_for_frames(3)
assert(not menu_visible("menu.file.new_song"), "Alt+F blocked by dialog")
vp:key("escape")
wait_for_frames(5)
vp:key("alt-f")
wait_for_frames(3)
assert(menu_visible("menu.file.new_song"), "Alt+F opens File menu")
-- Escape with a popup open: egui closes the popup, and since a popup is
-- not a dialog, the app handler falls through to the edit-mode toggle
-- (preserved old behavior: close_topmost_dialog finds no dialog).
vp:key("escape")
wait_for_frames(3)
assert(not menu_visible("menu.file.new_song"), "Escape closes File menu")
assert(get_widget_text("status.mode") == "VIEW", "Escape toggled EDT->VIEW")
-- Escape with menu bar active (no popup): menu consumes it, deactivates.
vp:key("escape")
wait_for_frames(3)
assert(get_widget_text("status.mode") == "VIEW", "menu-bar Escape keeps mode")
-- Escape with bar inactive: toggles back to EDT.
vp:key("escape")
wait_for_frames(3)
assert(get_widget_text("status.mode") == "EDT", "Escape toggles VIEW->EDT")
log("PASS")

-- 4. Pattern nav still works after dialog cycle (no stuck state)
log("--- nav after dialog cycle ---")
vp:key("down")
vp:key("up")
vp:key("right")
vp:key("left")
vp:key("tab")
wait_for_frames(3)
assert(get_widget_text("status.mode") == "EDT", "still EDT after nav")
vp:key("shift-tab")
wait_for_frames(3)
assert(get_widget_text("status.mode") == "EDT", "still EDT after shift-tab")
log("PASS")

-- 5. Newly registered P2 dialogs: Escape closes them without toggling mode.
-- SampleSelector ("Browse..." in the instrument sample-map header). If it
-- were missing from the registry, Escape would toggle EDT->VIEW.
log("--- SampleSelector dialog ---")
vp:key("f4")
wait_for_frames(5)
assert(get_widget_text("status.mode") == "EDT", "instrument view, EDT")
local browse = vp:widget_get("inst.map.browse")
browse:click()
wait_for_frames(5)
assert(get_widget_text("status.mode") == "EDT", "browser open, EDT")
vp:key("escape")
wait_for_frames(5)
assert(get_widget_text("status.mode") == "EDT", "Escape closed browser, mode kept")
log("PASS")

log("---")
log("All input-routing tests PASSED")
return { ok = true }
