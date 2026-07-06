-- Check if menu items appear on a different viewport after Alt+F
fixture("empty_project")
local vp = root()

-- List all viewports
local all_vps = viewports()
log("Viewports: " .. #all_vps)
for _, v in ipairs(all_vps) do
    local s = v:state()
    log("  id=" .. v.id .. " title=" .. tostring(s.title))
end

-- Try Alt+F and check ALL viewports
vp:key("alt-f")
wait_for_frames(3)

local after_vps = viewports()
log("After Alt+F, viewports: " .. #after_vps)
for _, v in ipairs(after_vps) do
    local s = v:state()
    log("  id=" .. v.id .. " title=" .. tostring(s.title))
    -- List all widgets in this viewport
    local widgets = v:widget_list({ include_invisible = true })
    log("    widgets: " .. #widgets)
    for _, w in ipairs(widgets) do
        local id_str = tostring(w.id)
        if string.find(id_str, "menu") then
            log("    MENU: " .. id_str)
        end
    end
end

-- Also check the root viewport for menu items
local root_widgets = vp:widget_list({ include_invisible = true })
log("Root viewport widgets: " .. #root_widgets)
local menu_count = 0
for _, w in ipairs(root_widgets) do
    local id_str = tostring(w.id)
    if string.find(id_str, "menu") then
        log("  ROOT MENU: " .. id_str)
        menu_count = menu_count + 1
    end
end
log("Root menu widgets: " .. menu_count)

return { vp_count = #after_vps, root_menus = menu_count }
