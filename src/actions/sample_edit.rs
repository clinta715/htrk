use std::sync::Arc;

use crate::app::HtrkApp;
use crate::edit::{
    SampleProperty, SetSampleDataCommand, SetSamplePropertyCommand,
};
use crate::ui::sample_editor::SampleEditEvent;

pub(crate) enum SelectionUpdate {
    Clear,
    Set(usize, usize),
}

pub(crate) fn handle_sample_edit(app: &mut HtrkApp, event: SampleEditEvent) -> Option<SelectionUpdate> {
    let sample_idx = app.core.selected_sample;
    let module = match &app.core.module {
        Some(m) => m,
        None => return None,
    };
    let sample = match module.samples.get(sample_idx) {
        Some(s) => s,
        None => return None,
    };

    let cmd: Box<dyn crate::edit::EditCommand> = match event {
        SampleEditEvent::NameChanged(n) => Box::new(SetSamplePropertyCommand {
            sample_index: sample_idx,
            property: SampleProperty::Name(n),
            old_property: SampleProperty::Name(sample.name.clone()),
        }),
        SampleEditEvent::VolumeChanged(v) => Box::new(SetSamplePropertyCommand {
            sample_index: sample_idx,
            property: SampleProperty::DefaultVolume(v),
            old_property: SampleProperty::DefaultVolume(sample.default_volume),
        }),
        SampleEditEvent::PanningChanged(p) => Box::new(SetSamplePropertyCommand {
            sample_index: sample_idx,
            property: SampleProperty::DefaultPanning(p),
            old_property: SampleProperty::DefaultPanning(sample.default_panning),
        }),
        SampleEditEvent::GlobalVolumeChanged(v) => Box::new(SetSamplePropertyCommand {
            sample_index: sample_idx,
            property: SampleProperty::GlobalVolume(v),
            old_property: SampleProperty::GlobalVolume(sample.global_volume),
        }),
        SampleEditEvent::LoopTypeChanged(t) => Box::new(SetSamplePropertyCommand {
            sample_index: sample_idx,
            property: SampleProperty::LoopType(t),
            old_property: SampleProperty::LoopType(sample.loop_type),
        }),
        SampleEditEvent::LoopStartChanged(s) => Box::new(SetSamplePropertyCommand {
            sample_index: sample_idx,
            property: SampleProperty::LoopStart(s),
            old_property: SampleProperty::LoopStart(sample.loop_start),
        }),
        SampleEditEvent::LoopEndChanged(e) => Box::new(SetSamplePropertyCommand {
            sample_index: sample_idx,
            property: SampleProperty::LoopEnd(e),
            old_property: SampleProperty::LoopEnd(sample.loop_end),
        }),
        SampleEditEvent::RelativeNoteChanged(n) => Box::new(SetSamplePropertyCommand {
            sample_index: sample_idx,
            property: SampleProperty::RelativeNote(n),
            old_property: SampleProperty::RelativeNote(sample.relative_note),
        }),
        SampleEditEvent::FineTuneChanged(t) => Box::new(SetSamplePropertyCommand {
            sample_index: sample_idx,
            property: SampleProperty::FineTune(t),
            old_property: SampleProperty::FineTune(sample.fine_tune),
        }),
        SampleEditEvent::Normalize => {
            let mut data = (*sample.data).clone();
            let max = data.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
            if max > 0.0 {
                let factor = 1.0 / max;
                for x in data.iter_mut() {
                    *x *= factor;
                }
            }
            Box::new(SetSampleDataCommand {
                sample_index: sample_idx,
                old_data: sample.data.clone(),
                new_data: Arc::new(data),
            })
        }
        SampleEditEvent::Reverse => {
            let mut data = (*sample.data).clone();
            data.reverse();
            Box::new(SetSampleDataCommand {
                sample_index: sample_idx,
                old_data: sample.data.clone(),
                new_data: Arc::new(data),
            })
        }
        SampleEditEvent::CutRegion(s, e) => {
            let s = s.min(e);
            let e = s.max(e);
            let mut data = (*sample.data).clone();
            app.sample_editor.clipboard = Some(Arc::new(data[s..e].to_vec()));
            data.drain(s..e);
            app.core.execute_edit_command(Box::new(SetSampleDataCommand {
                sample_index: sample_idx,
                old_data: sample.data.clone(),
                new_data: Arc::new(data),
            }));
            return Some(SelectionUpdate::Clear);
        }
        SampleEditEvent::CopyRegion(s, e) => {
            let s = s.min(e);
            let e = s.max(e);
            app.sample_editor.clipboard = Some(Arc::new(sample.data[s..e].to_vec()));
            return None;
        }
        SampleEditEvent::PasteRegion(pos) => {
            let clip = match app.sample_editor.clipboard.as_ref() {
                Some(c) => c.clone(),
                None => return None,
            };
            let data = (*sample.data).clone();
            let pos = pos.min(data.len());
            let mut new_data = Vec::with_capacity(data.len() + clip.len());
            new_data.extend_from_slice(&data[..pos]);
            new_data.extend_from_slice(&clip);
            new_data.extend_from_slice(&data[pos..]);
            Box::new(SetSampleDataCommand {
                sample_index: sample_idx,
                old_data: sample.data.clone(),
                new_data: Arc::new(new_data),
            })
        }
        SampleEditEvent::CropRegion(s, e) => {
            let s = s.min(e);
            let e = s.max(e);
            let new_len = e - s;
            let data = sample.data[s..e].to_vec();
            app.core.execute_edit_command(Box::new(SetSampleDataCommand {
                sample_index: sample_idx,
                old_data: sample.data.clone(),
                new_data: Arc::new(data),
            }));
            return Some(SelectionUpdate::Set(0, new_len));
        }
        SampleEditEvent::Amplify(factor) => {
            let mut data = (*sample.data).clone();
            for x in data.iter_mut() {
                *x *= factor;
            }
            Box::new(SetSampleDataCommand {
                sample_index: sample_idx,
                old_data: sample.data.clone(),
                new_data: Arc::new(data),
            })
        }
        SampleEditEvent::SilenceRegion(s, e) => {
            let s = s.min(e);
            let e = s.max(e);
            let mut data = (*sample.data).clone();
            for x in data[s..e].iter_mut() {
                *x = 0.0;
            }
            Box::new(SetSampleDataCommand {
                sample_index: sample_idx,
                old_data: sample.data.clone(),
                new_data: Arc::new(data),
            })
        }
        SampleEditEvent::TrimSilence => {
            let data = &sample.data;
            let threshold = 0.001;
            let start = data.iter().position(|&x| x.abs() > threshold).unwrap_or(0);
            let end = data.iter().rposition(|&x| x.abs() > threshold).map(|p| p + 1).unwrap_or(data.len());
            let trimmed = if start < end { data[start..end].to_vec() } else { Vec::new() };
            Box::new(SetSampleDataCommand {
                sample_index: sample_idx,
                old_data: sample.data.clone(),
                new_data: Arc::new(trimmed),
            })
        }
        SampleEditEvent::SetLoopFromSelection(start, end) => {
            let start_cmd: Box<dyn crate::edit::EditCommand> = Box::new(SetSamplePropertyCommand {
                sample_index: sample_idx,
                property: SampleProperty::LoopStart(start),
                old_property: SampleProperty::LoopStart(sample.loop_start),
            });
            let end_cmd: Box<dyn crate::edit::EditCommand> = Box::new(SetSamplePropertyCommand {
                sample_index: sample_idx,
                property: SampleProperty::LoopEnd(end),
                old_property: SampleProperty::LoopEnd(sample.loop_end),
            });
            let type_cmd: Box<dyn crate::edit::EditCommand> = Box::new(SetSamplePropertyCommand {
                sample_index: sample_idx,
                property: SampleProperty::LoopType(crate::sequencer::sample::LoopType::Forward),
                old_property: SampleProperty::LoopType(sample.loop_type),
            });
            app.core.execute_edit_commands(vec![start_cmd, end_cmd, type_cmd]);
            return None;
        }
        SampleEditEvent::ImportSample => {
            app.file_browser.open(crate::ui::file_browser::BrowserMode::Samples, crate::ui::file_browser::DialogMode::Open, &mut app.config);
            return None;
        }
        SampleEditEvent::ExportSample(idx) => {
            let module = match &app.core.module {
                Some(m) => m,
                None => return None,
            };
            let sample = match module.samples.get(idx) {
                Some(s) if !s.data.is_empty() => s,
                _ => return None,
            };
            let default_dir = app.config.default_wav_path.as_deref();
            let bit_depth = app.config.get_sample_export_bit_depth();
            app.sample_export_dialog = Some(
                crate::ui::sample_export_dialog::SampleExportDialog::new(
                    idx,
                    sample.name.clone(),
                    sample.sample_rate,
                    default_dir,
                    bit_depth,
                )
            );
            return None;
        }
        SampleEditEvent::SliceToInstrument => {
            app.slice_config.source_sample = sample_idx;
            app.slice_dialog_open = true;
            return None;
        }
        SampleEditEvent::FadeIn(s, e) => {
            let s = s.min(e);
            let e = s.max(e);
            let len = e.saturating_sub(s);
            if len == 0 { return None; }
            let mut data = (*sample.data).clone();
            for i in 0..len {
                let gain = i as f32 / len as f32;
                data[s + i] *= gain;
            }
            Box::new(SetSampleDataCommand {
                sample_index: sample_idx,
                old_data: sample.data.clone(),
                new_data: Arc::new(data),
            })
        }
        SampleEditEvent::FadeOut(s, e) => {
            let s = s.min(e);
            let e = s.max(e);
            let len = e.saturating_sub(s);
            if len == 0 { return None; }
            let mut data = (*sample.data).clone();
            for i in 0..len {
                let gain = 1.0 - (i as f32 / len as f32);
                data[s + i] *= gain;
            }
            Box::new(SetSampleDataCommand {
                sample_index: sample_idx,
                old_data: sample.data.clone(),
                new_data: Arc::new(data),
            })
        }
    };

    app.core.execute_edit_command(cmd);
    None
}
