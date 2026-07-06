-- Focused test: Press Alt+F, check if menu opens
fixture_raw("empty_project")
local vp = root()
vp:wait_for_settle()

-- Send Alt+F
vp:key("alt-f")

-- Wait for frame
wait_for_frames(3)

-- List ALL widgets (including invisible) to see if menu items appeared
local all = vp:widget_list({ include_invisible = true })
log("After Alt+F, total widgets: " .. #all)

local menu_count = 0
for _, w in ipairs(all) do
    local id_str = tostring(w.id)
    if string.find(id_str, "menu") then
        local s = w:state()
        log("MENU: id=" .. id_str .. " label=" .. tostring(s.label) .. " visible=" .. tostring(s.visible))
        menu_count = menu_count + 1
    end
end
log("Menu widgets found: " .. menu_count)

-- Also check view.* widgets
local view_count = 0
for _, w in ipairs(all) do
    local id_str = tostring(w.id)
    if string.find(id_str, "view") then
        local s = w:state()
        log("VIEW: id=" .. id_str .. " label=" .. tostring(s.label) .. " visible=" .. tostring(s.visible))
        view_count = view_count + 1
    end
end
log("View widgets found: " .. view_count)

return { total = #all, menus = menu_count, views = view_count }
