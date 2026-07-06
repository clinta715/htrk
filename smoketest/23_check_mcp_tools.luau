-- Check what MCP tools/resources are available from the sandbox

-- We can't list all tools directly, but we can call specific ones
local ok, result = pcall(function()
    return mcp.call("module.read", {})
end)
if ok then
    log("module.read succeeded, keys: ")
    local keys = {}
    for k, _ in pairs(result) do
        table.insert(keys, k)
    end
    table.sort(keys)
    for _, k in ipairs(keys) do
        log("  " .. k)
    end
else
    log("module.read failed: " .. tostring(result))
end

-- Try to read a pattern resource
local ok2, result2 = pcall(function()
    return mcp.call("resources.read", { uri = "patterns://" })
end)
if ok2 then
    log("patterns resource: type=" .. type(result2))
    if type(result2) == "table" then
        for k, v in pairs(result2) do
            log("  " .. tostring(k) .. " = " .. tostring(v))
        end
    end
else
    log("patterns resource failed: " .. tostring(result2))
end

-- Try list tools
local ok3, result3 = pcall(function()
    return mcp.call("tools.list", {})
end)
if ok3 then
    log("tools.list returned " .. #result3 .. " tools")
    for i, t in ipairs(result3) do
        log("  " .. tostring(i) .. ": " .. tostring(t.name or t))
    end
else
    log("tools.list failed: " .. tostring(result3))
end

return { module_read_ok = ok }
