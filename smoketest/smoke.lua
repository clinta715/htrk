-- Smoke test: app launches and basic widgets are visible
-- Verifies the main UI components render without errors.

local ok, result = pcall(function()
  -- Check that the view selector exists
  local pattern_btn = assert(registry.find("view.pattern"), "view.pattern not found")
  assert(pattern_btn.role == "selectable_value", "view.pattern role mismatch")
  assert(pattern_btn.value == "true", "view.pattern should be selected")

  -- Check transport bar controls
  assert(registry.find("transport.play"), "transport.play not found")
  assert(registry.find("transport.stop"), "transport.stop not found")
  assert(registry.find("transport.bpm"), "transport.bpm not found")
  assert(registry.find("transport.speed"), "transport.speed not found")
  assert(registry.find("transport.volume"), "transport.volume not found")

  -- Check status bar labels
  assert(registry.find("status.version"), "status.version not found")
  assert(registry.find("status.mode"), "status.mode not found")
  assert(registry.find("status.format"), "status.format not found")
end)

if not ok then
  error("smoke.lua failed: " .. tostring(result))
end
