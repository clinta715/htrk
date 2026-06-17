-- Smoke test: instrument editor layout
-- Verifies the 2-column settings grid, maps row, and envelope section
-- are all present and wired after switching to the instrument view.

local ok, result = pcall(function()
  -- Switch to instrument view via fixture
  fixture("instrument_view")

  -- === Header ===
  assert(registry.find("inst.header.name"), "inst.header.name not found")
  assert(registry.find("inst.header.save"), "inst.header.save not found")
  assert(registry.find("inst.header.load"), "inst.header.load not found")

  -- === Settings Grid — Left Column ===

  -- General group
  assert(registry.find("inst.general.vol"), "inst.general.vol not found")

  -- Pitch-Pan group
  assert(registry.find("inst.pitchpan.sep"), "inst.pitchpan.sep not found")
  assert(registry.find("inst.pitchpan.center"), "inst.pitchpan.center not found")

  -- Filter group
  assert(registry.find("inst.filter.cutoff"), "inst.filter.cutoff not found")
  assert(registry.find("inst.filter.res"), "inst.filter.res not found")
  assert(registry.find("inst.filter.type.lp"), "inst.filter.type.lp not found")
  assert(registry.find("inst.filter.type.hp"), "inst.filter.type.hp not found")
  assert(registry.find("inst.filter.type.bp"), "inst.filter.type.bp not found")
  assert(registry.find("inst.filter.type.notch"), "inst.filter.type.notch not found")

  -- === Settings Grid — Right Column ===

  -- NNA group
  assert(registry.find("inst.nna.cut"), "inst.nna.cut not found")
  assert(registry.find("inst.nna.cont"), "inst.nna.cont not found")
  assert(registry.find("inst.nna.off"), "inst.nna.off not found")
  assert(registry.find("inst.nna.fade"), "inst.nna.fade not found")
  assert(registry.find("inst.nna.dct_label"), "inst.nna.dct_label not found")
  assert(registry.find("inst.nna.dct.off"), "inst.nna.dct.off not found")
  assert(registry.find("inst.nna.dct.note"), "inst.nna.dct.note not found")
  assert(registry.find("inst.nna.dct.samp"), "inst.nna.dct.samp not found")
  assert(registry.find("inst.nna.dct.inst"), "inst.nna.dct.inst not found")
  assert(registry.find("inst.nna.dna_label"), "inst.nna.dna_label not found")
  assert(registry.find("inst.nna.dna.cut"), "inst.nna.dna.cut not found")
  assert(registry.find("inst.nna.dna.off"), "inst.nna.dna.off not found")
  assert(registry.find("inst.nna.dna.fade"), "inst.nna.dna.fade not found")

  -- Random group
  assert(registry.find("inst.random.vol"), "inst.random.vol not found")
  assert(registry.find("inst.random.pan"), "inst.random.pan not found")
  assert(registry.find("inst.random.flt"), "inst.random.flt not found")

  -- Vibrato group
  assert(registry.find("inst.vib.type.sine"), "inst.vib.type.sine not found")
  assert(registry.find("inst.vib.type.ramp"), "inst.vib.type.ramp not found")
  assert(registry.find("inst.vib.type.sq"), "inst.vib.type.sq not found")
  assert(registry.find("inst.vib.type.rand"), "inst.vib.type.rand not found")
  assert(registry.find("inst.vib.sweep"), "inst.vib.sweep not found")
  assert(registry.find("inst.vib.depth"), "inst.vib.depth not found")
  assert(registry.find("inst.vib.rate"), "inst.vib.rate not found")

  -- === Maps Row ===
  assert(registry.find("inst.map.paint_label"), "inst.map.paint_label not found")
  assert(registry.find("inst.map.browse"), "inst.map.browse not found")
  assert(registry.find("inst.map.fill_all"), "inst.map.fill_all not found")

  -- === Envelope Section ===
  assert(registry.find("inst.env.tab.vol"), "inst.env.tab.vol not found")
  assert(registry.find("inst.env.tab.pan"), "inst.env.tab.pan not found")
  assert(registry.find("inst.env.tab.pit"), "inst.env.tab.pit not found")
  assert(registry.find("inst.env.tab.flt"), "inst.env.tab.flt not found")

  assert(registry.find("inst.env.enabled"), "inst.env.enabled not found")
  assert(registry.find("inst.env.sustain"), "inst.env.sustain not found")
  assert(registry.find("inst.env.carry"), "inst.env.carry not found")

  assert(registry.find("inst.env.loop"), "inst.env.loop not found")

  -- Toolbar buttons
  assert(registry.find("inst.env.add_point"), "inst.env.add_point not found")
  assert(registry.find("inst.env.generate"), "inst.env.generate not found")

  -- === Check widget roles ===
  local vol_widget = registry.find("inst.general.vol")
  assert(vol_widget.role == "slider", "inst.general.vol should be slider, got " .. tostring(vol_widget.role))

  local name_widget = registry.find("inst.header.name")
  assert(name_widget.role == "text_edit", "inst.header.name should be text_edit, got " .. tostring(name_widget.role))

  local save_widget = registry.find("inst.header.save")
  assert(save_widget.role == "button", "inst.header.save should be button, got " .. tostring(save_widget.role))

  local lp_widget = registry.find("inst.filter.type.lp")
  assert(lp_widget.role == "selectable", "inst.filter.type.lp should be selectable, got " .. tostring(lp_widget.role))

  -- === Verify interactivity — browse button click ===
  local browse_btn = registry.find("inst.map.browse")
  browse_btn:click()
end)

if not ok then
  error("instrument_editor.lua failed: " .. tostring(result))
end
