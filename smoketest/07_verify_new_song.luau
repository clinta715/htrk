-- Verify Ctrl+N creates a module by checking status bar state
fixture_raw("empty_project")
local vp = root()
vp:wait_for_settle()

-- Check initial format status
local fmt_widget = vp:widget_get("status.format")
local fmt_before = fmt_widget:state().value_text
log("Format before: '" .. fmt_before .. "'")

-- Send Ctrl+N
vp:key("ctrl-n")
wait_for_frames(3)

-- Check format after
local fmt_after = fmt_widget:state().value_text
log("Format after: '" .. fmt_after .. "'")

-- Check total widget count
local all = vp:widget_list({ include_invisible = true })
log("Widgets after: " .. #all)

-- Also check if view.toggle widgets exist
local view_check = pcall(function()
    local w = vp:widget_get("view.pattern")
    log("view.pattern: " .. w:state().value_text)
end)
if view_check then
    log("view.pattern EXISTS!")
else
    log("view.pattern STILL MISSING")
end

-- Check for any widget containing "view" in id
for _, w in ipairs(all) do
    local id_str = tostring(w.id)
    if string.find(id_str, "view") then
        log("Found view widget: " .. id_str)
    end
    if string.find(id_str, "menu") then
        log("Found menu widget: " .. id_str)
    end
end

return { fmt_before = fmt_before, fmt_after = fmt_after }
