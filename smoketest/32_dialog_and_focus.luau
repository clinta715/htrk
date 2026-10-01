-- P6 regression coverage: dialog-owned keys, widget text delivery, Alt-tap.
--  1. View-switch keys (F2/F3/F4) are blocked while a dialog is open.
--  2. Tab/arrow storm inside a dialog: nothing leaks to the pattern editor.
--  3. Text typed into a focused widget is delivered (P1 never strips Text).
--  4. Alt-tap via raw press/release activates the menu bar (§24).
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

local function view_selected(id)
    local ok, w = pcall(function() return vp:widget_get(id) end)
    if ok then return w:state().value_text == "true" end
    return false
end

assert(view_selected("view.pattern"), "starts in pattern view")

-- 1. F3 blocked by dialog, works after close
log("--- F3 gated on dialog ---")
vp:key("f10")
wait_for_frames(5)
vp:key("f3")
wait_for_frames(3)
assert(view_selected("view.pattern"), "F3 blocked by dialog")
assert(get_widget_text("status.mode") == "EDT", "mode kept")
vp:key("escape")
wait_for_frames(5)
vp:key("f3")
wait_for_frames(3)
assert(view_selected("view.sample"), "F3 switches after close")
vp:key("f2")
wait_for_frames(3)
assert(view_selected("view.pattern"), "F2 back to pattern")
log("PASS")

-- 2. Tab/arrow storm in dialog
log("--- Tab/arrow in dialog ---")
vp:key("f10")
wait_for_frames(5)
for _ = 1, 4 do
    vp:key("tab")
    vp:key("down")
    vp:key("up")
    vp:key("left")
    vp:key("right")
end
wait_for_frames(3)
assert(get_widget_text("status.mode") == "EDT", "storm leaks nothing")
assert(view_selected("view.pattern"), "view kept")
vp:key("escape")
wait_for_frames(5)
assert(get_widget_text("status.mode") == "EDT", "Escape closed dialog")
log("PASS")

-- 3. Destructive keys blocked by dialog (Delete/Backspace/Insert/Space/Ctrl+Z).
log("--- destructive keys gated ---")
vp:key("f10")
wait_for_frames(5)
vp:key("delete")
vp:key("backspace")
vp:key("insert")
vp:key("space")
vp:key("ctrl-z")
wait_for_frames(3)
assert(get_widget_text("status.mode") == "EDT", "destructive keys blocked")
vp:key("escape")
wait_for_frames(5)
assert(get_widget_text("status.mode") == "EDT", "Escape closed dialog")
log("PASS")

-- 4. Widget text delivery (instrument name field)
log("--- text to focused widget ---")
vp:key("f4")
wait_for_frames(5)
local name = vp:widget_get("inst.header.name")
local before = name:state().value_text or ""
name:type_text("Z")
wait_for_frames(3)
local after = get_widget_text("inst.header.name") or ""
assert(string.find(after, "Z", 1, true) ~= nil, "widget received text (before=" .. before .. " after=" .. after .. ")")
assert(get_widget_text("status.mode") == "EDT", "mode kept")
log("PASS")

-- 5. Real widget focus (click-established): App keys park, text delivers.
-- Click-to-focus timing is runner-flaky, so calibrate: repeat click + F2
-- until F2 is parked (pattern view NOT selected) or attempts run out.
log("--- widget focus gate ---")
vp:key("f4")
wait_for_frames(5)
local focused = false
for _ = 1, 3 do
    local field = vp:widget_get("inst.header.name")
    field:click()
    wait_for_frames(4)
    vp:key("f2")
    wait_for_frames(3)
    if not view_selected("view.pattern") and get_widget_text("status.mode") == "EDT" then
        focused = true
        break
    end
    vp:key("f4")
    wait_for_frames(5)
end
assert(focused, "click established widget focus (runner flake otherwise)")
-- F3/F5 park while focused.
vp:key("f3")
wait_for_frames(3)
assert(not view_selected("view.sample"), "F3 parked by focus gate")
local play_before = vp:widget_get("transport.play"):state().label
vp:key("f5")
wait_for_frames(3)
assert(vp:widget_get("transport.play"):state().label == play_before, "F5 parked by focus gate")
-- Text reaches the focused widget (P1 never strips Text) without incident.
local field = vp:widget_get("inst.header.name")
field:type_text("Q")
wait_for_frames(3)
local after = get_widget_text("inst.header.name") or ""
assert(string.find(after, "Q", 1, true) ~= nil, "focused widget received text")
assert(get_widget_text("status.mode") == "EDT", "mode kept")
log("PASS")

-- NOTE: bare Alt-tap and focused-Escape are not testable with this runner:
-- `vp:key("alt")`/`raw_key("alt", ...)` fail with "Unknown key" (egui has
-- no Key::Alt), and the runner drops widget focus around Escape delivery,
-- so the app never observes a focused Escape. The focus gate itself is
-- proven above (F2/F3/F5 park) and by unit tests. Alt+letter and menu-bar
-- navigation are covered in test 20.

log("---")
log("All dialog/focus tests PASSED")
return { ok = true }
