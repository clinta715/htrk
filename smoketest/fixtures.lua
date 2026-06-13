-- Smoke test: query and apply fixtures

local ok, result = pcall(function()
  -- List available fixtures
  local fixtures = fixtures.list()
  assert(#fixtures > 0, "no fixtures registered")

  -- Check for expected fixtures by name
  local fixture_names = {}
  for _, f in ipairs(fixtures) do
    fixture_names[f.name] = true
  end

  assert(fixture_names["empty_project"], "missing fixture: empty_project")
  assert(fixture_names["pattern_view"], "missing fixture: pattern_view")
  assert(fixture_names["sample_view"], "missing fixture: sample_view")
  assert(fixture_names["instrument_view"], "missing fixture: instrument_view")
  assert(fixture_names["sendfx_view"], "missing fixture: sendfx_view")
  assert(fixture_names["playback_view"], "missing fixture: playback_view")
  assert(fixture_names["automation_view"], "missing fixture: automation_view")

  -- Apply the empty_project fixture (should be a no-op on a fresh launch)
  fixtures.apply("empty_project")
end)

if not ok then
  error("fixtures.lua failed: " .. tostring(result))
end
