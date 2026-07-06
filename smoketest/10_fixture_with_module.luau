-- Test: empty_project fixture now creates a module
fixture("empty_project")
local vp = root()

-- After fixture, view.pattern should appear
local found = vp:widget_get("view.pattern")
local s = found:state()
log("view.pattern: label=" .. tostring(s.label) .. " visible=" .. tostring(s.visible))

-- All view tabs should exist
local views = { "view.pattern", "view.sample", "view.instrument", "view.sendfx", "view.playback", "view.automation", "view.mixer" }
for _, id in ipairs(views) do
    local ok = pcall(function()
        local w = vp:widget_get(id)
        log(id .. " exists")
    end)
    assert(ok, id .. " should exist")
end

-- Check format in status bar
local fmt = vp:widget_get("status.format")
log("Format: " .. fmt:state().value_text)
assert(string.find(fmt:state().value_text, "Fmt:") ~= nil, "Format should show a format type (module created)")

-- Check total widget count (should be much more than 23)
local all = vp:widget_list({ include_invisible = true })
log("Total widgets: " .. #all)
assert(#all > 30, "Widget count should increase with module loaded (got " .. #all .. ")")

-- Check widget list has new types
local pat_btn = vp:widget_get("pattern.add_channel")
log("Pattern widget check: " .. pat_btn:state().label)

log("---")
log("Fixture with module verification PASSED")
return { widgets = #all, fmt = fmt:state().value_text }
