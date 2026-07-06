use std::collections::HashMap;
use std::sync::Arc;

use crate::app::HtrkApp;
use crate::edit::{
    AddEnvelopePointCommand, EnvelopeType, InstrumentProperty,
    MapNoteToSampleCommand, MapNoteToNoteCommand, RemoveEnvelopePointCommand,
    SetEnvelopeFlagsCommand, SetEnvelopeLoopCommand, SetEnvelopePointCommand,
    SetEnvelopePointsCommand, SetEnvelopeSustainCommand, SetInstrumentPropertyCommand,
};
use crate::sequencer::instrument::EnvelopePoint;
use crate::ui::instrument_editor::InstrumentEditEvent;

pub(crate) fn handle_instrument_edit(app: &mut HtrkApp, event: InstrumentEditEvent) {
    let inst_idx = app.core.selected_instrument;
    let module = match &app.core.module {
        Some(m) => m,
        None => return,
    };
    let inst = match module.instruments.get(inst_idx) {
        Some(i) => i,
        None => return,
    };

    let cmd: Box<dyn crate::edit::EditCommand> = match event {
        InstrumentEditEvent::NameChanged(n) => Box::new(SetInstrumentPropertyCommand {
            instrument_index: inst_idx,
            property: InstrumentProperty::Name(n),
            old_property: InstrumentProperty::Name(inst.name.clone()),
        }),
        InstrumentEditEvent::NnaChanged(n) => Box::new(SetInstrumentPropertyCommand {
            instrument_index: inst_idx,
            property: InstrumentProperty::Nna(n),
            old_property: InstrumentProperty::Nna(inst.nna),
        }),
        InstrumentEditEvent::DuplicateCheckTypeChanged(t) => Box::new(SetInstrumentPropertyCommand {
            instrument_index: inst_idx,
            property: InstrumentProperty::DuplicateCheckType(t),
            old_property: InstrumentProperty::DuplicateCheckType(inst.duplicate_check_type),
        }),
        InstrumentEditEvent::DuplicateCheckActionChanged(a) => Box::new(SetInstrumentPropertyCommand {
            instrument_index: inst_idx,
            property: InstrumentProperty::DuplicateCheckAction(a),
            old_property: InstrumentProperty::DuplicateCheckAction(inst.duplicate_check_action),
        }),
        InstrumentEditEvent::FadeoutChanged(f) => Box::new(SetInstrumentPropertyCommand {
            instrument_index: inst_idx,
            property: InstrumentProperty::Fadeout(f),
            old_property: InstrumentProperty::Fadeout(inst.fade_out),
        }),
        InstrumentEditEvent::GlobalVolumeChanged(v) => Box::new(SetInstrumentPropertyCommand {
            instrument_index: inst_idx,
            property: InstrumentProperty::GlobalVolume(v),
            old_property: InstrumentProperty::GlobalVolume(inst.global_volume),
        }),
        InstrumentEditEvent::PitchPanSeparationChanged(s) => Box::new(SetInstrumentPropertyCommand {
            instrument_index: inst_idx,
            property: InstrumentProperty::PitchPanSeparation(s),
            old_property: InstrumentProperty::PitchPanSeparation(inst.pitch_pan_separation),
        }),
        InstrumentEditEvent::PitchPanCenterChanged(c) => Box::new(SetInstrumentPropertyCommand {
            instrument_index: inst_idx,
            property: InstrumentProperty::PitchPanCenter(c),
            old_property: InstrumentProperty::PitchPanCenter(inst.pitch_pan_center),
        }),
        InstrumentEditEvent::RandomVolumeChanged(v) => Box::new(SetInstrumentPropertyCommand {
            instrument_index: inst_idx,
            property: InstrumentProperty::RandomVolume(v),
            old_property: InstrumentProperty::RandomVolume(inst.random_volume),
        }),
        InstrumentEditEvent::RandomPanningChanged(p) => Box::new(SetInstrumentPropertyCommand {
            instrument_index: inst_idx,
            property: InstrumentProperty::RandomPanning(p),
            old_property: InstrumentProperty::RandomPanning(inst.random_panning),
        }),
        InstrumentEditEvent::FilterCutoffChanged(c) => Box::new(SetInstrumentPropertyCommand {
            instrument_index: inst_idx,
            property: InstrumentProperty::FilterCutoff(c),
            old_property: InstrumentProperty::FilterCutoff(inst.filter_cutoff),
        }),
        InstrumentEditEvent::FilterResonanceChanged(r) => Box::new(SetInstrumentPropertyCommand {
            instrument_index: inst_idx,
            property: InstrumentProperty::FilterResonance(r),
            old_property: InstrumentProperty::FilterResonance(inst.filter_resonance),
        }),
        InstrumentEditEvent::FilterTypeChanged(t) => Box::new(SetInstrumentPropertyCommand {
            instrument_index: inst_idx,
            property: InstrumentProperty::FilterType(t),
            old_property: InstrumentProperty::FilterType(inst.filter_type),
        }),
        InstrumentEditEvent::FilterRandomCutoffChanged(c) => Box::new(SetInstrumentPropertyCommand {
            instrument_index: inst_idx,
            property: InstrumentProperty::FilterRandomCutoff(c),
            old_property: InstrumentProperty::FilterRandomCutoff(inst.filter_random_cutoff),
        }),
        InstrumentEditEvent::EnvelopePointMoved(env_type, idx, t, v) => {
            let env = inst.envelope(env_type);
            let old_pt = env.as_ref().map(|e| e.points[idx]).unwrap_or_default();
            Box::new(SetEnvelopePointCommand {
                instrument_index: inst_idx,
                envelope_type: env_type,
                point_index: idx,
                old_point: old_pt,
                new_point: EnvelopePoint { tick: t, value: v },
            })
        }
        InstrumentEditEvent::EnvelopePointAdded(env_type, t, v) => Box::new(AddEnvelopePointCommand {
            instrument_index: inst_idx,
            envelope_type: env_type,
            point: EnvelopePoint { tick: t, value: v },
        }),
        InstrumentEditEvent::EnvelopePointRemoved(env_type, idx) => {
            let env = inst.envelope(env_type);
            let old_pt = env.as_ref().map(|e| e.points[idx]).unwrap_or_default();
            Box::new(RemoveEnvelopePointCommand {
                instrument_index: inst_idx,
                envelope_type: env_type,
                point_index: idx,
                old_point: old_pt,
            })
        }
        InstrumentEditEvent::EnvelopeSustainChanged(env_type, new_sustain) => {
            let env = inst.envelope(env_type);
            Box::new(SetEnvelopeSustainCommand {
                instrument_index: inst_idx,
                envelope_type: env_type,
                old_sustain: env.as_ref().and_then(|e| e.sustain_point),
                new_sustain,
            })
        }
        InstrumentEditEvent::EnvelopeLoopChanged(env_type, new_enabled, new_start, new_end) => {
            let env = inst.envelope(env_type);
            Box::new(SetEnvelopeLoopCommand {
                instrument_index: inst_idx,
                envelope_type: env_type,
                old_loop_enabled: env.as_ref().map_or(false, |e| e.flags.loop_),
                new_loop_enabled: new_enabled,
                old_loop_start: env.as_ref().and_then(|e| e.loop_start),
                new_loop_start: new_start,
                old_loop_end: env.as_ref().and_then(|e| e.loop_end),
                new_loop_end: new_end,
            })
        }
        InstrumentEditEvent::EnvelopeFlagsChanged(env_type, new_flags) => {
            let env = inst.envelope(env_type);
            Box::new(SetEnvelopeFlagsCommand {
                instrument_index: inst_idx,
                envelope_type: env_type,
                old_flags: env.as_ref().map(|e| e.flags).unwrap_or_default(),
                new_flags,
            })
        }
        InstrumentEditEvent::GenerateEnvelope(env_type, points) => {
            let envelope = match env_type {
                EnvelopeType::Volume => &inst.volume_envelope,
                EnvelopeType::Panning => &inst.panning_envelope,
                EnvelopeType::Pitch => &inst.pitch_envelope,
                EnvelopeType::Filter => &inst.filter_envelope,
            };
            Box::new(SetEnvelopePointsCommand {
                instrument_index: inst_idx,
                envelope_type: env_type,
                new_points: points,
                old_points: envelope.as_ref().map(|e| e.points.clone()).unwrap_or_default(),
                old_envelope: envelope.as_ref().cloned(),
            })
        }
        InstrumentEditEvent::SampleMapChanged(note, new_idx) => Box::new(MapNoteToSampleCommand {
            instrument_index: inst_idx,
            note,
            old_sample: inst.sample_map[note as usize],
            new_sample: new_idx,
        }),
        InstrumentEditEvent::NoteMapChanged(note, new_dest) => Box::new(MapNoteToNoteCommand {
            instrument_index: inst_idx,
            note,
            old_dest: inst.note_map[note as usize],
            new_dest,
        }),
        InstrumentEditEvent::VibTypeChanged(v) => Box::new(SetInstrumentPropertyCommand {
            instrument_index: inst_idx,
            property: InstrumentProperty::VibType(v),
            old_property: InstrumentProperty::VibType(inst.vib_type),
        }),
        InstrumentEditEvent::VibSweepChanged(v) => Box::new(SetInstrumentPropertyCommand {
            instrument_index: inst_idx,
            property: InstrumentProperty::VibSweep(v),
            old_property: InstrumentProperty::VibSweep(inst.vib_sweep),
        }),
        InstrumentEditEvent::VibDepthChanged(v) => Box::new(SetInstrumentPropertyCommand {
            instrument_index: inst_idx,
            property: InstrumentProperty::VibDepth(v),
            old_property: InstrumentProperty::VibDepth(inst.vib_depth),
        }),
        InstrumentEditEvent::VibRateChanged(v) => Box::new(SetInstrumentPropertyCommand {
            instrument_index: inst_idx,
            property: InstrumentProperty::VibRate(v),
            old_property: InstrumentProperty::VibRate(inst.vib_rate),
        }),
        InstrumentEditEvent::SampleMapFillAll(sample_idx) => Box::new(
            crate::edit::SetSampleMapCommand {
                instrument_index: inst_idx,
                new_sample_index: sample_idx,
                old_map: inst.sample_map,
            },
        ),
        InstrumentEditEvent::SaveInstrument => {
            return;
        }
        InstrumentEditEvent::LoadInstrument => {
            return;
        }
        InstrumentEditEvent::ExportInstrument(_) => {
            return;
        }
        InstrumentEditEvent::ImportInstrument => {
            return;
        }
        InstrumentEditEvent::PluginUnload => {
            return;
        }
        InstrumentEditEvent::OpenPluginEditor => {
            return;
        }
        InstrumentEditEvent::ClosePluginEditor => {
            return;
        }
    };

    app.core.execute_edit_command(cmd);
}

pub(crate) fn save_instrument_to_file(app: &mut HtrkApp, inst_idx: usize, path: &str) {
    let module = match &app.core.module {
        Some(m) => m,
        None => return,
    };
    let inst = match module.instruments.get(inst_idx) {
        Some(i) => i,
        None => return,
    };
    let sample_indices: Vec<u8> = inst.sample_map.iter().cloned().collect();
    let samples: Vec<_> = sample_indices.iter()
        .filter_map(|&idx| {
            if idx > 0 && (idx as usize) < module.samples.len() {
                Some(module.samples[idx as usize].clone())
            } else {
                None
            }
        })
        .collect();
    let data = match crate::formats::hti::save_instrument(inst, &samples) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to save instrument: {:?}", e);
            return;
        }
    };
    if let Err(e) = std::fs::write(path, &data) {
        eprintln!("Failed to write instrument file: {}", e);
    }
}

pub(crate) fn load_instrument_from_file(app: &mut HtrkApp, path: &str) {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to read instrument file: {}", e);
            return;
        }
    };
    let (loaded_inst, loaded_samples) = match crate::formats::hti::load_instrument(&data) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("Failed to load instrument: {:?}", e);
            return;
        }
    };
    let inst_idx = app.core.selected_instrument;
    if app.core.module.is_none() {
        app.new_song();
    }
    app.core.ensure_module_ownership();
    if let Some(ref mut module_arc) = app.core.module {
        if let Some(m) = Arc::get_mut(module_arc) {
            if inst_idx >= m.instruments.len() {
                m.instruments.resize(inst_idx + 1, crate::sequencer::Instrument::default());
            }
            let sample_map = loaded_inst.sample_map.clone();
            m.instruments[inst_idx] = loaded_inst;
            let mut available_slots: Vec<usize> = (1..m.samples.len())
                .filter(|&i| m.samples[i].data.is_empty())
                .collect();
            let mut sample_mapping: HashMap<usize, usize> = HashMap::new();
            for (new_idx, sample) in loaded_samples.iter().enumerate() {
                if let Some(&existing_idx) = available_slots.first() {
                    m.samples[existing_idx] = sample.clone();
                    sample_mapping.insert(new_idx + 1, existing_idx);
                    available_slots.remove(0);
                } else {
                    let new_sample_idx = m.samples.len();
                    m.samples.push(sample.clone());
                    sample_mapping.insert(new_idx + 1, new_sample_idx);
                }
            }
            let mut remapped_map = [0u8; 120];
            for (note, &old_idx) in sample_map.iter().enumerate().take(120) {
                if let Some(&new_idx) = sample_mapping.get(&(old_idx as usize)) {
                    remapped_map[note] = new_idx as u8;
                } else {
                    remapped_map[note] = old_idx;
                }
            }
            m.instruments[inst_idx].sample_map = remapped_map;
        }
    }
    app.core.sync_module_to_audio();
}
