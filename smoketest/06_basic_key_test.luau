-- Absolute minimal test: does key injection affect the UI at all?
fixture_raw("empty_project")
local vp = root()
vp:wait_for_settle()

-- Get initial viewport state
local vs1 = vp:state()
log("Initial frame: " .. vs1.frame_count)

-- Send a simple key
vp:key("escape")
wait_for_frames(2)

local vs2 = vp:state()
log("After escape: frame=" .. vs2.frame_count)

-- Try clicking a transport button
local ok = pcall(function()
    local btn = vp:widget_get("transport.play")
    log("Play button: " .. btn:state().value_text)
    btn:click()
end)
if ok then log("Click succeeded") else log("Click failed") end

wait_for_frames(2)

local vs3 = vp:state()
log("After click: frame=" .. vs3.frame_count)

return { f1 = vs1.frame_count, f2 = vs2.frame_count, f3 = vs3.frame_count }
