-- Smoke test: menu bar has expected menus and items

local ok, result = pcall(function()
  -- Check top-level menus exist
  assert(registry.find("menu.file"), "menu.file not found")
  assert(registry.find("menu.edit"), "menu.edit not found")
  assert(registry.find("menu.view"), "menu.view not found")
  assert(registry.find("menu.audio"), "menu.audio not found")
  assert(registry.find("menu.help"), "menu.help not found")
end)

if not ok then
  error("menu_bar.lua failed: " .. tostring(result))
end
