-- Explore widget structure
fixture_raw("empty_project")
local vp = root()
vp:wait_for_settle()

-- List visible widgets
local visible = vp:widget_list()
log("Visible: " .. #visible)
for _, w in ipairs(visible) do
    local s = w:state()
    log("widget:")
    log("  id=" .. tostring(w.id))
    log("  label=" .. tostring(s.label))
    log("  val=" .. s.value_text)
    log("  vis=" .. tostring(s.visible))
end

-- List ALL
local all = vp:widget_list({ include_invisible = true })
log("ALL: " .. #all)
for _, w in ipairs(all) do
    local s = w:state()
    log("[" .. (if s.visible then "V" else "H") .. "] " .. tostring(w.id))
end

-- view.pattern check
local ok = pcall(function()
    local w = vp:widget_get("view.pattern")
    local s = w:state()
    log("view.pattern exists, visible=" .. tostring(s.visible))
end)
if not ok then log("view.pattern absent") end

-- menu items
local m = vp:widget_list({ id_prefix = "menu" })
log("menu-prefix: " .. #m)
for _, w in ipairs(m) do
    log("  " .. tostring(w.id))
end

return { count = #visible }
