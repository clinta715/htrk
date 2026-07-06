-- Explore widgets to find cursor position indicators
-- Need to register the fixture in each script
fixture("empty_project", function()
    return mcp.call("fixture.apply_fixture", { name = "empty_project" })
end)
fixture("empty_project")
local vp = root()

-- List all widgets to find cursor-related ones
local all = vp:widget_list({ include_invisible = false })
log("Total visible widgets: " .. #all)

for _, w in ipairs(all) do
    local id_str = tostring(w.id)
    local s = w:state()
    local text = s.value_text or ""
    local id_lower = string.lower(id_str)
    if string.find(id_lower, "cursor") or string.find(id_lower, "position") 
       or string.find(id_lower, "row") or string.find(id_lower, "col")
       or string.find(id_lower, "status")
       or string.find(id_lower, "order") then
        log("  " .. id_str .. " = \"" .. text .. "\"")
    end
end

log("---Named widgets---")
for _, w in ipairs(all) do
    local id_str = tostring(w.id)
    local s = w:state()
    if not string.find(id_str, "^[0-9a-f]+$") then
        local text = s.value_text or ""
        log("  ID: " .. id_str .. " text=\"" .. text .. "\"")
    end
end

log("---status labels---")
local status_entries = vp:widget_list({ id_prefix = "status", include_invisible = true })
for _, w in ipairs(status_entries) do
    log("  " .. tostring(w.id) .. " = \"" .. (w:state().value_text or "") .. "\"")
end

return { total = #all }
