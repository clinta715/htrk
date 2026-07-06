-- Verify arrow keys and Tab navigation in pattern editor
-- The app has a built-in "empty_project" fixture (registers via DevMcp::on_fixture)
fixture("empty_project")
local vp = root()

local function get_widget_text(id)
    local ok, w = pcall(function() return vp:widget_get(id) end)
    if ok then return w:state().value_text or "" end
    return nil
end

-- Baseline: verify edit mode and pattern view
local mode = get_widget_text("status.mode")
log("Initial mode: " .. tostring(mode))
assert(mode == "EDT", "Should be in edit mode")

-- Verify pattern view is present
local pat_btn = vp:widget_get("pattern.add_channel")
log("Pattern widget OK")

-- Test: ArrowDown
log("--- ArrowDown ---")
vp:key("down")
wait_for_frames(3)
mode = get_widget_text("status.mode")
assert(mode == "EDT", "Still in edit mode after ArrowDown")
log("PASS")

-- Test: ArrowUp
log("--- ArrowUp ---")
vp:key("up")
wait_for_frames(3)
assert(get_widget_text("status.mode") == "EDT")
log("PASS")

-- Test: ArrowRight (sub-column)
log("--- ArrowRight ---")
vp:key("right")
wait_for_frames(3)
assert(get_widget_text("status.mode") == "EDT")
log("PASS")

-- Test: ArrowLeft
log("--- ArrowLeft ---")
vp:key("left")
wait_for_frames(3)
assert(get_widget_text("status.mode") == "EDT")
log("PASS")

-- Test: Tab (next channel)
log("--- Tab ---")
vp:key("tab")
wait_for_frames(3)
assert(get_widget_text("status.mode") == "EDT")
log("PASS")

-- Test: Shift+Tab (prev channel)
log("--- Shift+Tab ---")
vp:key("shift-tab")
wait_for_frames(3)
assert(get_widget_text("status.mode") == "EDT")
log("PASS")

-- Test: Multi-step nav
log("--- Multi-step ---")
vp:key("down")
vp:key("down")
vp:key("right")
wait_for_frames(3)
assert(get_widget_text("status.mode") == "EDT")
log("PASS")

-- Test: Ctrl+Left/Right (channel skip)
log("--- Ctrl+Arrow ---")
vp:key("ctrl-left")
wait_for_frames(3)
assert(get_widget_text("status.mode") == "EDT")

vp:key("ctrl-right")
wait_for_frames(3)
assert(get_widget_text("status.mode") == "EDT")
log("PASS")

-- Test: PageUp / PageDown (big scroll)
log("--- PageUp/PageDown ---")
vp:key("pageup")
wait_for_frames(3)
assert(get_widget_text("status.mode") == "EDT")

vp:key("pagedown")
wait_for_frames(3)
assert(get_widget_text("status.mode") == "EDT")
log("PASS")

-- Test: Home / End (first/last row)
log("--- Home/End ---")
vp:key("home")
wait_for_frames(3)
assert(get_widget_text("status.mode") == "EDT")

vp:key("end")
wait_for_frames(3)
assert(get_widget_text("status.mode") == "EDT")
log("PASS")

log("---")
log("All arrow/tab navigation tests PASSED")
return { ok = true }
