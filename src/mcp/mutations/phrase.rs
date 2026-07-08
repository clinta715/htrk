//! Phrase-generation mutation handler: `phrase.generate`.

use std::sync::Arc;

use crate::core::HtrkCore;
use crate::edit::BulkSetCellsCommand;
use crate::mcp::protocol::CmdResult;
use crate::sequencer::pattern::MAX_CHANNELS;

use super::common::{get_f64, get_i64, get_str};

pub(super) fn parse_scale(s: &str) -> Result<crate::tools::scale::Scale, String> {
    use crate::tools::scale::Scale;
    match s {
        "chromatic" => Ok(Scale::Chromatic),
        "major" => Ok(Scale::Major),
        "natural_minor" | "natural minor" | "minor" => Ok(Scale::NaturalMinor),
        "harmonic_minor" | "harmonic minor" => Ok(Scale::HarmonicMinor),
        "pentatonic_minor" | "pentatonic minor" => Ok(Scale::PentatonicMinor),
        "pentatonic_major" | "pentatonic major" => Ok(Scale::PentatonicMajor),
        "blues" => Ok(Scale::Blues),
        "dorian" => Ok(Scale::Dorian),
        "phrygian" => Ok(Scale::Phrygian),
        _ => Err(format!("Unknown scale: '{s}'")),
    }
}

fn parse_gen_mode(s: &str) -> Result<crate::tools::phrase_generator::GenMode, String> {
    use crate::tools::phrase_generator::GenMode;
    match s {
        "melodic" => Ok(GenMode::Melodic),
        "euclidean" => Ok(GenMode::Euclidean),
        "drum" => Ok(GenMode::Drum),
        "chord" => Ok(GenMode::Chord),
        _ => Err(format!("Unknown mode: '{s}'")),
    }
}

fn parse_chord_type(s: &str) -> Result<crate::tools::phrase_generator::ChordType, String> {
    use crate::tools::phrase_generator::ChordType;
    match s {
        "triad" => Ok(ChordType::Triad),
        "seventh" | "7th" => Ok(ChordType::Seventh),
        "sus2" => Ok(ChordType::Sus2),
        "sus4" => Ok(ChordType::Sus4),
        _ => Err(format!("Unknown chord type: '{s}'")),
    }
}

fn parse_progression(s: &str) -> Result<crate::tools::phrase_generator::Progression, String> {
    use crate::tools::phrase_generator::Progression;
    match s {
        "I-IV-V-I" | "one_four_five_one" => Ok(Progression::OneFourFiveOne),
        "I-V-vi-IV" | "one_five_six_four" => Ok(Progression::OneFiveSixFour),
        "I-vi-IV-V" | "one_six_four_five" => Ok(Progression::OneSixFourFive),
        "I-iii-IV-V" | "one_three_four_five" => Ok(Progression::OneThreeFourFive),
        "circle" | "circle_of_fifths" => Ok(Progression::Circle),
        _ => Err(format!("Unknown progression: '{s}'")),
    }
}

pub(super) fn cmd_phrase_generate(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let mode_str = get_str!(params, "mode").ok_or("Missing 'mode'")?;
    let mode = parse_gen_mode(&mode_str)?;

    let order = get_i64!(params, "order").unwrap_or_else(|| core.selected_order as i64) as usize;
    let start_row = get_i64!(params, "start_row").unwrap_or(0) as usize;
    let end_row = get_i64!(params, "end_row").unwrap_or(63) as usize;

    let scale_str = get_str!(params, "scale").unwrap_or_else(|| "major".into());
    let scale = parse_scale(&scale_str)?;
    let root = get_i64!(params, "root").unwrap_or(0) as u8;
    let octave_min = get_i64!(params, "octave_min").unwrap_or(3) as u8;
    let octave_max = get_i64!(params, "octave_max").unwrap_or(5) as u8;
    let density = get_f64!(params, "density").unwrap_or(0.3) as f32;
    let step_size = get_i64!(params, "step_size").unwrap_or(3) as u8;
    let seed = get_i64!(params, "seed").unwrap_or(0) as u64;
    let instrument = get_i64!(params, "instrument").map(|v| v as u8);
    let pulses = get_i64!(params, "pulses").unwrap_or(8) as usize;
    let rotation = get_i64!(params, "rotation").unwrap_or(0) as usize;
    let kick_ch = get_i64!(params, "kick_ch").unwrap_or(0) as usize;
    let snare_ch = get_i64!(params, "snare_ch").unwrap_or(1) as usize;
    let hat_ch = get_i64!(params, "hat_ch").unwrap_or(2) as usize;

    // Per-drum instruments and densities (drum mode only). Any of these
    // may be `None` to fall back to the shared `instrument` / default
    // density, so old single-instrument drum calls keep working.
    let kick_instrument = get_i64!(params, "kick_instrument").map(|v| v as u8);
    let snare_instrument = get_i64!(params, "snare_instrument").map(|v| v as u8);
    let hat_instrument = get_i64!(params, "hat_instrument").map(|v| v as u8);
    let kick_density = get_f64!(params, "kick_density").map(|v| v as f32);
    let snare_density = get_f64!(params, "snare_density").map(|v| v as f32);
    let hat_density = get_f64!(params, "hat_density").map(|v| v as f32);
    let swing = get_f64!(params, "swing").unwrap_or(0.0) as f32;

    let chord_type_str = get_str!(params, "chord_type").unwrap_or_else(|| "triad".into());
    let chord_type = parse_chord_type(&chord_type_str)?;
    let progression_str = get_str!(params, "progression").unwrap_or_else(|| "I-IV-V-I".into());
    let progression = parse_progression(&progression_str)?;
    let bars_per_chord = get_i64!(params, "bars_per_chord").unwrap_or(4) as u8;
    let chord_channels = params.get("chord_channels")
        .and_then(|v| v.as_array())
        .map(|arr| {
            let mut chs = [0usize; 4];
            for (i, v) in arr.iter().enumerate().take(4) {
                if let Some(n) = v.as_i64() {
                    chs[i] = n as usize;
                }
            }
            chs
        })
        .unwrap_or([0, 1, 2, 3]);

    let phrase_params = crate::tools::phrase_generator::PhraseParams {
        mode,
        scale,
        root,
        octave_min,
        octave_max,
        density,
        step_size,
        seed,
        instrument,
        pulses,
        rotation,
        kick_ch,
        snare_ch,
        hat_ch,
        kick_instrument,
        snare_instrument,
        hat_instrument,
        kick_density,
        snare_density,
        hat_density,
        swing,
        chord_type,
        progression,
        bars_per_chord,
        chord_channels,
    };

    let num_channels = core.num_channels();
    if num_channels == 0 {
        return Err("No channels in module".into());
    }

    let notes = crate::tools::phrase_generator::generate_phrase(
        &phrase_params, start_row, end_row, num_channels,
    );

    if notes.is_empty() {
        return Ok(serde_json::json!({
            "ok": true, "cells_set": 0, "pattern": order,
            "notes_placed": {},
        }));
    }

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            // Auto-extend the order list if the requested order doesn't exist yet.
            if order >= arc_module.order_list.len() {
                let fallback_pat: u8 = if arc_module.patterns.is_empty() { 0 }
                                   else { (arc_module.patterns.len() - 1) as u8 };
                while arc_module.order_list.len() <= order {
                    arc_module.order_list.push(fallback_pat);
                }
            }
            let pat_idx = arc_module.order_list[order] as usize;
            if pat_idx >= arc_module.patterns.len() {
                return Err(format!("Pattern index {pat_idx} does not exist"));
            }
            let pattern = &arc_module.patterns[pat_idx];

            let mut old_cells = Vec::new();
            let mut new_cells = Vec::new();
            // Per-channel placement counter for the response.
            let mut per_channel: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
            for &(row, ch, cell) in &notes {
                if row < pattern.num_rows && ch < MAX_CHANNELS {
                    old_cells.push((row, ch, pattern.data[row][ch]));
                    new_cells.push((row, ch, cell));
                    *per_channel.entry(ch).or_insert(0) += 1;
                }
            }

            if old_cells.is_empty() {
                return Ok(serde_json::json!({
                    "ok": true, "cells_set": 0, "pattern": order,
                }));
            }

            let cells_set = new_cells.len();
            let label = format!("phrase.generate {mode_str}");
            let cmd = Box::new(BulkSetCellsCommand {
                order,
                old_cells,
                new_cells,
            });
            let undo_id = core.undo_manager.execute_with_label(cmd, label.clone(), arc_module)
                .map_err(|e| format!("Phrase apply failed: {e:?}"))?;
            core.sync_module_to_audio();
            return Ok(serde_json::json!({
                "ok": true,
                "cells_set": cells_set,
                "undo_id": undo_id,
                "undo_label": label,
                "pattern": order,
                "notes_placed": per_channel,
            }));
        }
    }
    Err("No module loaded".into())
}
