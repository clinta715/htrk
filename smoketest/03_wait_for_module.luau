-- Test: press Ctrl+N and wait for view.pattern to appear
fixture_raw("empty_project")
local vp = root()
vp:wait_for_settle()
log("Initial widgets: " .. #vp:widget_list())

-- Try keyboard
vp:key("ctrl-n")
log("Sent Ctrl+N")

-- Wait for view.pattern with up to 15s timeout
local found = vp:wait_for_widget("view.pattern", function(w)
    return w.visible == true
end)
if found ~= nil then
    log("view.pattern appeared after Ctrl+N! label=" .. tostring(found.label))
else
    log("TIMEOUT: view.pattern did not appear")
end

-- Also check all widgets after waiting
local all = vp:widget_list()
log("After wait, widgets: " .. #all)
for _, w in ipairs(all) do
    log("  " .. tostring(w.id))
end

return { appeared = found ~= nil }
