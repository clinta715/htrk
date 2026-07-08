//! Instrument mutation handlers: create / remove / set_property /
//! map_note / map_range.

use std::sync::Arc;

use crate::core::HtrkCore;
use crate::edit::{MapNoteToSampleCommand, SetInstrumentPropertyCommand, SetSampleMapCommand, InstrumentProperty};
use crate::mcp::protocol::CmdResult;
use crate::sequencer::effect::FilterType;
use crate::sequencer::instrument::Instrument;

use super::common::{get_i64, get_str};

pub(super) fn cmd_instrument_create(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let name = get_str!(params, "name").unwrap_or_default();
    let base_sample = get_i64!(params, "base_sample").map(|v| v as u8);

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if arc_module.instruments.len() >= 256 {
                return Err("Maximum instruments reached (256)".into());
            }
            let mut inst = Instrument::default();
            inst.name = name;
            if let Some(sample_idx) = base_sample {
                for i in 0..120 {
                    inst.sample_map[i] = sample_idx;
                }
            }
            let idx = arc_module.instruments.len();
            arc_module.instruments.push(inst);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true, "index": idx}));
        }
    }
    Err("No module loaded".into())
}

pub(super) fn cmd_instrument_remove(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let idx = get_i64!(params, "index").ok_or("Missing 'index'")? as usize;
    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if idx >= arc_module.instruments.len() {
                return Err(format!("Instrument {idx} out of range"));
            }
            if arc_module.instruments.len() <= 1 {
                return Err("Cannot remove the last instrument".into());
            }
            let name = arc_module.instruments[idx].name.clone();
            arc_module.instruments.remove(idx);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true, "removed_index": idx, "name": name}));
        }
    }
    Err("No module loaded".into())
}

pub(super) fn cmd_instrument_set_property(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let idx = get_i64!(params, "index").ok_or("Missing 'index'")? as usize;
    let prop_name = get_str!(params, "property").ok_or("Missing 'property'")?;
    let value = params.get("value").ok_or("Missing 'value'")?;

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if idx >= arc_module.instruments.len() {
                return Err(format!("Instrument {idx} out of range"));
            }
            let inst = &arc_module.instruments[idx];
            let (property, old_property): (InstrumentProperty, InstrumentProperty) = match prop_name.as_str() {
                "name" => {
                    let v = value.as_str().ok_or("'value' must be a string")?.to_string();
                    (InstrumentProperty::Name(v.clone()), InstrumentProperty::Name(inst.name.clone()))
                }
                "fade_out" | "fadeout" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u16;
                    (InstrumentProperty::Fadeout(v), InstrumentProperty::Fadeout(inst.fade_out))
                }
                "global_volume" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (InstrumentProperty::GlobalVolume(v), InstrumentProperty::GlobalVolume(inst.global_volume))
                }
                "nna" => {
                    let s = value.as_str().ok_or("'value' must be a string")?;
                    let v = match s {
                        "cut" | "note_cut" => crate::sequencer::instrument::NewNoteAction::NoteCut,
                        "continue" => crate::sequencer::instrument::NewNoteAction::Continue,
                        "off" | "note_off" => crate::sequencer::instrument::NewNoteAction::NoteOff,
                        "fade" | "note_fade" => crate::sequencer::instrument::NewNoteAction::NoteFade,
                        _ => return Err(format!("Unknown NNA: '{s}'")),
                    };
                    (InstrumentProperty::Nna(v), InstrumentProperty::Nna(inst.nna))
                }
                "dct" | "duplicate_check_type" => {
                    let s = value.as_str().ok_or("'value' must be a string")?;
                    let v = match s {
                        "disabled" => crate::sequencer::instrument::DuplicateCheckType::Disabled,
                        "note" => crate::sequencer::instrument::DuplicateCheckType::Note,
                        "sample" => crate::sequencer::instrument::DuplicateCheckType::Sample,
                        "instrument" => crate::sequencer::instrument::DuplicateCheckType::Instrument,
                        _ => return Err(format!("Unknown DCT: '{s}'")),
                    };
                    (InstrumentProperty::DuplicateCheckType(v), InstrumentProperty::DuplicateCheckType(inst.duplicate_check_type))
                }
                "dca" | "duplicate_check_action" => {
                    let s = value.as_str().ok_or("'value' must be a string")?;
                    let v = match s {
                        "cut" | "note_cut" => crate::sequencer::instrument::DuplicateCheckAction::NoteCut,
                        "off" | "note_off" => crate::sequencer::instrument::DuplicateCheckAction::NoteOff,
                        "fade" | "note_fade" => crate::sequencer::instrument::DuplicateCheckAction::NoteFade,
                        _ => return Err(format!("Unknown DCA: '{s}'")),
                    };
                    (InstrumentProperty::DuplicateCheckAction(v), InstrumentProperty::DuplicateCheckAction(inst.duplicate_check_action))
                }
                "pitch_pan_separation" | "pitch_pan_sep" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as i8;
                    (InstrumentProperty::PitchPanSeparation(v), InstrumentProperty::PitchPanSeparation(inst.pitch_pan_separation))
                }
                "pitch_pan_center" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (InstrumentProperty::PitchPanCenter(v), InstrumentProperty::PitchPanCenter(inst.pitch_pan_center))
                }
                "random_volume" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (InstrumentProperty::RandomVolume(v), InstrumentProperty::RandomVolume(inst.random_volume))
                }
                "random_panning" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (InstrumentProperty::RandomPanning(v), InstrumentProperty::RandomPanning(inst.random_panning))
                }
                "filter_cutoff" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u16;
                    (InstrumentProperty::FilterCutoff(v), InstrumentProperty::FilterCutoff(inst.filter_cutoff))
                }
                "filter_resonance" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (InstrumentProperty::FilterResonance(v), InstrumentProperty::FilterResonance(inst.filter_resonance))
                }
                "filter_type" => {
                    let s = value.as_str().ok_or("'value' must be a string")?;
                    let v = match s {
                        "lowpass" | "lp" => FilterType::LowPass,
                        "highpass" | "hp" => FilterType::HighPass,
                        "bandpass" | "bp" => FilterType::BandPass,
                        "notch" => FilterType::Notch,
                        _ => return Err(format!("Unknown filter type: '{s}'")),
                    };
                    (InstrumentProperty::FilterType(v), InstrumentProperty::FilterType(inst.filter_type))
                }
                "filter_random_cutoff" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (InstrumentProperty::FilterRandomCutoff(v), InstrumentProperty::FilterRandomCutoff(inst.filter_random_cutoff))
                }
                "vib_type" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (InstrumentProperty::VibType(v), InstrumentProperty::VibType(inst.vib_type))
                }
                "vib_sweep" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (InstrumentProperty::VibSweep(v), InstrumentProperty::VibSweep(inst.vib_sweep))
                }
                "vib_depth" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (InstrumentProperty::VibDepth(v), InstrumentProperty::VibDepth(inst.vib_depth))
                }
                "vib_rate" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (InstrumentProperty::VibRate(v), InstrumentProperty::VibRate(inst.vib_rate))
                }
                _ => return Err(format!("Unknown instrument property: '{prop_name}'")),
            };
            let cmd = Box::new(SetInstrumentPropertyCommand { instrument_index: idx, property, old_property });
            let _ = core.undo_manager.execute(cmd, arc_module);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true}));
        }
    }
    Err("No module loaded".into())
}

fn note_name_to_midi_key(s: &str) -> Result<u8, String> {
    let tone_names = ["C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-"];
    let s_upper = s.to_uppercase();
    if s_upper.len() < 3 {
        if let Ok(k) = s_upper.parse::<u8>() {
            if k <= 119 { return Ok(k); }
        }
        return Err(format!("Invalid note: '{s}'"));
    }
    let tone_str = &s_upper[..2];
    let octave_str = &s_upper[2..];
    let tone = tone_names.iter().position(|&t| t == tone_str)
        .ok_or_else(|| format!("Unknown note name: '{s}'"))?;
    let octave = octave_str.parse::<u8>().map_err(|_| format!("Invalid octave in '{s}'"))?;
    let key = octave * 12 + tone as u8;
    if key > 119 { return Err(format!("Note '{s}' out of range")); }
    Ok(key)
}

pub(super) fn cmd_instrument_map_note(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let idx = get_i64!(params, "index").ok_or("Missing 'index'")? as usize;
    let note_str = get_str!(params, "note").ok_or("Missing 'note'")?;
    let sample_idx = get_i64!(params, "sample_index").ok_or("Missing 'sample_index'")? as u8;
    let note = note_name_to_midi_key(&note_str)?;

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if idx >= arc_module.instruments.len() {
                return Err(format!("Instrument {idx} out of range"));
            }
            let old_sample = arc_module.instruments[idx].sample_map[note as usize];
            let cmd = Box::new(MapNoteToSampleCommand { instrument_index: idx, note, old_sample, new_sample: sample_idx });
            let _ = core.undo_manager.execute(cmd, arc_module);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true, "note": note_str, "sample": sample_idx}));
        }
    }
    Err("No module loaded".into())
}

pub(super) fn cmd_instrument_map_range(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let idx = get_i64!(params, "index").ok_or("Missing 'index'")? as usize;
    let note_start_str = get_str!(params, "note_start").ok_or("Missing 'note_start'")?;
    let note_end_str = get_str!(params, "note_end").ok_or("Missing 'note_end'")?;
    let sample_idx = get_i64!(params, "sample_index").ok_or("Missing 'sample_index'")? as u8;
    let note_start = note_name_to_midi_key(&note_start_str)?;
    let note_end = note_name_to_midi_key(&note_end_str)?;
    if note_start > note_end {
        return Err("note_start must be <= note_end".into());
    }

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if idx >= arc_module.instruments.len() {
                return Err(format!("Instrument {idx} out of range"));
            }
            let old_map = arc_module.instruments[idx].sample_map;
            let cmd = Box::new(SetSampleMapCommand { instrument_index: idx, new_sample_index: sample_idx, old_map });
            let _ = core.undo_manager.execute(cmd, arc_module);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true, "notes_affected": (note_end - note_start + 1) as u64, "sample": sample_idx}));
        }
    }
    Err("No module loaded".into())
}
