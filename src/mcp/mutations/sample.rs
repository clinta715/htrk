//! Sample mutation handlers: load / remove / set_property, plus the
//! `sample_library.import` tool.

use std::sync::Arc;

use crate::core::HtrkCore;
use crate::edit::{SetSamplePropertyCommand, SampleProperty};
use crate::mcp::protocol::CmdResult;
use crate::sequencer::sample::LoopType;

use super::common::{get_i64, get_str};

pub(super) fn cmd_sample_load(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let path = get_str!(params, "path").ok_or("Missing 'path'")?;
    let name = get_str!(params, "name");
    let wav_data = std::fs::read(&path).map_err(|e| format!("Failed to read '{path}': {e}"))?;
    let mut sample = crate::formats::wav::import_wav(&wav_data)
        .map_err(|e| format!("Failed to import WAV: {e}"))?;
    if let Some(n) = name {
        sample.name = n;
    } else {
        if let Some(stem) = std::path::Path::new(&path).file_stem().and_then(|s| s.to_str()) {
            sample.name = stem.to_string();
        }
    }

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            let idx = arc_module.samples.len();
            arc_module.samples.push(sample);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true, "index": idx, "path": path}));
        }
    }
    Err("No module loaded".into())
}

pub(super) fn cmd_sample_remove(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let idx = get_i64!(params, "index").ok_or("Missing 'index'")? as usize;
    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if idx >= arc_module.samples.len() {
                return Err(format!("Sample {idx} out of range"));
            }
            let name = arc_module.samples[idx].name.clone();
            arc_module.samples.remove(idx);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true, "removed_index": idx, "name": name}));
        }
    }
    Err("No module loaded".into())
}

pub(super) fn cmd_sample_set_property(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let idx = get_i64!(params, "index").ok_or("Missing 'index'")? as usize;
    let prop_name = get_str!(params, "property").ok_or("Missing 'property'")?;
    let value = params.get("value").ok_or("Missing 'value'")?;

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if idx >= arc_module.samples.len() {
                return Err(format!("Sample {idx} out of range"));
            }
            let sample = &arc_module.samples[idx];
            let (property, old_property): (SampleProperty, SampleProperty) = match prop_name.as_str() {
                "name" => {
                    let v = value.as_str().ok_or("'value' must be a string")?.to_string();
                    (SampleProperty::Name(v.clone()), SampleProperty::Name(sample.name.clone()))
                }
                "default_volume" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (SampleProperty::DefaultVolume(v), SampleProperty::DefaultVolume(sample.default_volume))
                }
                "default_panning" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (SampleProperty::DefaultPanning(v), SampleProperty::DefaultPanning(sample.default_panning))
                }
                "global_volume" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (SampleProperty::GlobalVolume(v), SampleProperty::GlobalVolume(sample.global_volume))
                }
                "loop_type" => {
                    let s = value.as_str().ok_or("'value' must be a string")?;
                    let v = match s {
                        "none" | "off" => LoopType::None,
                        "forward" | "fw" => LoopType::Forward,
                        "pingpong" | "pp" => LoopType::PingPong,
                        "backward" | "bw" => LoopType::Backward,
                        _ => return Err(format!("Unknown loop type: '{s}'")),
                    };
                    (SampleProperty::LoopType(v), SampleProperty::LoopType(sample.loop_type))
                }
                "loop_start" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as usize;
                    (SampleProperty::LoopStart(v), SampleProperty::LoopStart(sample.loop_start))
                }
                "loop_end" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as usize;
                    (SampleProperty::LoopEnd(v), SampleProperty::LoopEnd(sample.loop_end))
                }
                "relative_note" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as i8;
                    (SampleProperty::RelativeNote(v), SampleProperty::RelativeNote(sample.relative_note))
                }
                "fine_tune" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as i8;
                    (SampleProperty::FineTune(v), SampleProperty::FineTune(sample.fine_tune))
                }
                _ => return Err(format!("Unknown sample property: '{prop_name}'")),
            };
            let cmd = Box::new(SetSamplePropertyCommand { sample_index: idx, property, old_property });
            let _ = core.undo_manager.execute(cmd, arc_module);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true}));
        }
    }
    Err("No module loaded".into())
}

/// Convert a note name like `C4` / `D#3` to a sample `relative_note`
/// (MIDI key relative to middle-C 60).
fn note_to_relative(note: &str) -> Result<i8, String> {
    let bytes = note.as_bytes();
    if bytes.len() < 2 {
        return Err("Invalid note".into());
    }
    let letter = bytes[0].to_ascii_uppercase();
    let semitones = match letter {
        b'C' => 0,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        b'B' => 11,
        _ => return Err("Invalid note letter".into()),
    };
    let mut idx = 1;
    let mut semitone_offset: i8 = 0;
    if idx < bytes.len() && (bytes[idx] == b'#' || bytes[idx] == b'-') {
        if bytes[idx] == b'#' {
            semitone_offset = 1;
        }
        idx += 1;
    }
    if idx >= bytes.len() {
        return Err("Missing octave".into());
    }
    let octave_str = &note[idx..];
    let octave: i8 = octave_str.parse().map_err(|_| "Invalid octave")?;
    let midi_key = 12 * (octave + 1) + semitones + semitone_offset;
    Ok(midi_key - 60)
}

pub(super) fn cmd_sample_library_import(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let path = get_str!(params, "path").ok_or("Missing 'path'")?;
    let name_override = get_str!(params, "name");
    let target_slot = get_i64!(params, "target_slot").map(|v| v as usize);
    let set_note = get_str!(params, "set_note");

    let wav_data = std::fs::read(&path).map_err(|e| format!("Failed to read '{path}': {e}"))?;
    let mut sample = crate::formats::wav::import_wav(&wav_data)
        .map_err(|e| format!("Failed to import WAV: {e}"))?;

    if let Some(n) = name_override {
        sample.name = n;
    } else if let Some(stem) = std::path::Path::new(&path).file_stem().and_then(|s| s.to_str()) {
        sample.name = stem.to_string();
    }

    if let Some(note_str) = set_note {
        if let Ok(rel) = note_to_relative(&note_str) {
            sample.relative_note = rel;
        }
    }

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            let idx = if let Some(slot) = target_slot {
                if slot < arc_module.samples.len() {
                    arc_module.samples[slot] = sample;
                    slot
                } else {
                    while arc_module.samples.len() <= slot {
                        arc_module.samples.push(crate::sequencer::sample::Sample::default());
                    }
                    arc_module.samples[slot] = sample;
                    slot
                }
            } else {
                let idx = arc_module.samples.len();
                arc_module.samples.push(sample);
                idx
            };
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true, "sample_index": idx, "path": path}));
        }
    }
    Err("No module loaded".into())
}
