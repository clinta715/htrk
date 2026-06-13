-- Smoke test: order list panel renders and basic operations work

local ok, result = pcall(function()
  -- Verify order list heading and buttons
  assert(registry.find("order_list.heading"), "order_list.heading not found")
  assert(registry.find("order_list.insert"), "order_list.insert not found")
  assert(registry.find("order_list.duplicate"), "order_list.duplicate not found")
  assert(registry.find("order_list.delete"), "order_list.delete not found")

  -- Click insert order button
  local insert_btn = registry.find("order_list.insert")
  assert(insert_btn, "order_list.insert not found")
  insert_btn:click()
end)

if not ok then
  error("order_list.lua failed: " .. tostring(result))
end
