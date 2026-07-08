
use std::sync::atomic::Ordering;

use crate::app::HtrkApp;
use crate::formats;
use crate::audio::renderer::WavRenderer;

pub(crate) fn load_file(app: &mut HtrkApp, path: &str) {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to read file: {}", e);
            return;
        }
    };

    if data.len() < 4 {
        eprintln!("File too small to be a valid module");
        return;
    }

    match formats::load_module(&data) {
        Ok(mut module) => {
            if module.samples.len() < 65 {
                module.samples.resize(65, crate::sequencer::Sample::default());
            }
            if module.instruments.len() < 17 {
                module.instruments.resize(17, crate::sequencer::Instrument::default());
            }

            let name = module.name.clone();
            app.core.load_module(module, name, Some(path.to_string()));
            app.pattern_view.scroll_row = 0;
            app.pattern_view.scroll_channel = 0;
            app.core.sync_channel_fields();
            app.sync_send_bus_state();
            // Restore any send-bus plugin slots in the loaded module.
            // The plugin files must be discoverable in the scan paths.
            app.sync_send_bus_plugin_state();
            // Restore any instrument plugin slots in the loaded module.
            // The plugin files must be discoverable in the scan paths.
            app.sync_instrument_plugin_state();

            add_recent_file(app, path);
        }
        Err(e) => {
            eprintln!("Failed to load module: {}", e);
        }
    }
}

pub(crate) fn add_recent_file(app: &mut HtrkApp, path: &str) {
    let path_str = path.to_string();
    app.config.recent_files.retain(|p| p != &path_str);
    app.config.recent_files.insert(0, path_str);
    app.config.recent_files.truncate(10);
    app.config.save();
}

pub(crate) fn import_wav(app: &mut HtrkApp, path: &str) {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to read WAV file: {}", e);
            return;
        }
    };
    match crate::formats::wav::import_wav(&data) {
        Ok(mut sample) => {
            if sample.name.is_empty() {
                if let Some(name) = std::path::Path::new(path).file_stem().and_then(|s| s.to_str()) {
                    sample.name = name.to_string();
                }
            }

            if app.core.module.is_none() {
                app.new_song();
            }

            let sample_idx = app.core.selected_sample;
            app.core.import_wav_to_sample(sample_idx, sample);

            app.core.with_module_mut(|arc_module, core| {
                let inst_idx = core.selected_instrument;
                if inst_idx > 0 && inst_idx < arc_module.instruments.len() {
                    for i in 0..120 {
                        arc_module.instruments[inst_idx].sample_map[i] = sample_idx as u8;
                    }
                }
            });
        }
        Err(e) => {
            eprintln!("Failed to import WAV: {}", e);
        }
    }
}

/// Import a Standard MIDI File (`.mid`/`.midi`) into the current song.
///
/// Each MIDI track becomes one tracker channel; timing is quantized to a
/// `rows_per_beat` grid (default 4 = 16th notes). The resulting patterns are
/// appended to `module.patterns` and spliced into the order list immediately
/// after the user's current `selected_order` position, so playback continues
/// from the current spot into the imported material.
///
/// Like `load_file`, this is **not undoable** — it's a structural merge.
pub(crate) fn import_midi(app: &mut HtrkApp, path: &str) {
    import_midi_with_opts(app, path, 4);
}

/// Import MIDI with an explicit `rows_per_beat` quantization grid.
pub(crate) fn import_midi_with_opts(app: &mut HtrkApp, path: &str, rows_per_beat: u32) {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to read MIDI file: {}", e);
            return;
        }
    };

    let imported = match crate::formats::midi::import_midi(&data, rows_per_beat) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("Failed to import MIDI: {}", e);
            return;
        }
    };

    if imported.patterns.is_empty() {
        eprintln!("MIDI import produced no patterns (empty file?)");
        return;
    }

    if app.core.module.is_none() {
        app.new_song();
    }

    let stem = std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("MIDI import")
        .to_string();

    app.core.with_module_mut(|module, core| {
        // Base index where the new patterns will live.
        let base = module.patterns.len();
        if base + imported.patterns.len() > 256 {
            eprintln!(
                "MIDI import needs {} pattern slots but only {} are free (max 256); truncating",
                imported.patterns.len(),
                256usize.saturating_sub(base)
            );
        }
        let take = imported.patterns.len().min(256usize.saturating_sub(base));
        for p in imported.patterns.into_iter().take(take) {
            module.patterns.push(p);
        }

        // Splice the new pattern indices into the order list right after the
        // current position so playback flows into the imported material.
        let insert_at = (core.selected_order + 1).min(module.order_list.len());
        for (i, off) in imported.order_offsets.iter().enumerate().take(take) {
            let pat_idx = base + i;
            if pat_idx <= u8::MAX as usize {
                module.order_list.insert(insert_at + i, pat_idx as u8);
            }
            let _ = off; // off == i by construction
        }

        // Honor the MIDI tempo only if the current module is at the default
        // tempo (don't clobber a deliberate user tempo).
        if imported.bpm > 0
            && module.initial_bpm == crate::sequencer::module::DEFAULT_BPM
        {
            module.initial_bpm = imported.bpm;
        }

        // Make sure channel_volume / channel_panning are wide enough for the
        // imported channels.
        let need = imported.channels_used.max(module.channel_panning.len());
        if module.channel_panning.len() < need {
            module.channel_panning.resize(need, crate::sequencer::module::PANNING_CENTER);
        }
        if module.channel_volume.len() < need {
            module.channel_volume.resize(need, crate::sequencer::module::VOLUME_MAX);
        }

        let _ = stem;
        if imported.tracks_skipped > 0 {
            eprintln!(
                "MIDI import: {} tracks skipped (beyond {}-channel limit)",
                imported.tracks_skipped, crate::sequencer::pattern::MAX_CHANNELS
            );
        }
    });

    // Re-size per-channel state arrays to match the new channel count.
    app.core.sync_channel_fields();
    add_recent_file(app, path);
}

pub(crate) fn save_current_file(app: &mut HtrkApp) {
    let path = match &app.core.file_path {
        Some(p) => p.clone(),
        None => {
            app.save_as_dialog();
            return;
        }
    };
    save_file_inner(app, &path);
}

fn save_file_inner(app: &mut HtrkApp, path: &str) {
    // Capture state from any loaded CLAP send-bus and instrument plugins
    // into the module's PluginSlot.state field so the file round-trips
    // faithfully.
    app.save_all_send_bus_plugin_states();
    app.save_all_instrument_plugin_states();
    app.core.save_file(path);
    add_recent_file(app, path);
}

pub(crate) fn open_wav_export_dialog(app: &mut HtrkApp) {
    let module_loaded = app.core.module.is_some();
    let total_orders = app.core.module.as_ref().map(|m| m.order_list.len()).unwrap_or(0) as u64;
    let sample_rate = if app.current_sample_rate > 0 {
        app.current_sample_rate
    } else {
        44100
    };

    let default_name = if !app.core.loaded_module_name.is_empty() {
        app.core.loaded_module_name.clone()
    } else {
        "untitled".to_string()
    };

    app.wav_export_state.default_directory = app.config.default_wav_path.as_ref().map(|p| {
        let pb = std::path::PathBuf::from(p);
        if pb.is_dir() { pb } else { std::path::PathBuf::new() }
    }).filter(|p| p.as_os_str().is_empty().then(|| false).unwrap_or(true));

    app.wav_export_state.open(&default_name, module_loaded, Some(total_orders), sample_rate);
    app.wav_export_state.update_estimates(Some(total_orders));
}

pub(crate) fn export_wav_with_settings(app: &mut HtrkApp) {
    let settings = app.wav_export_state.settings().clone();

    let module = match &app.core.module {
        Some(m) => m.clone(),
        None => {
            app.wav_export_state.finish_export(false, Some("No module loaded to export".to_string()));
            return;
        }
    };

    let file_path = match settings.file_path.clone() {
        Some(path) => path,
        None => {
            app.wav_export_state.finish_export(false, Some("No file path selected".to_string()));
            return;
        }
    };

    let sample_rate = if settings.sample_rate > 0 {
        settings.sample_rate
    } else {
        app.current_sample_rate
    };

    if sample_rate == 0 {
        app.wav_export_state.finish_export(false, Some("No valid sample rate available".to_string()));
        return;
    }

    let progress_arc = app.wav_export_state.progress_arc();
    let state_arc = app.wav_export_state.state_arc();
    let cancel_arc = app.wav_export_state.cancel_arc();

    let interp_str = app.config.default_interpolation.clone();
    let limiter_str = app.config.limiter_mode.clone();

    app.wav_export_state.start_export();

    std::thread::spawn(move || {
        let spec = hound::WavSpec {
            channels: settings.channel_mode.channels(),
            sample_rate,
            bits_per_sample: settings.bit_depth.bits(),
            sample_format: if settings.bit_depth.is_float() {
                hound::SampleFormat::Float
            } else {
                hound::SampleFormat::Int
            },
        };

        let result = std::fs::File::create(&file_path)
            .and_then(|file| {
                let writer = hound::WavWriter::new(file, spec)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
                Ok(writer)
            });

        match result {
            Ok(mut writer) => {
                let mut renderer = WavRenderer::new(module, sample_rate);
                let interp = match interp_str.as_str() {
                    "Nearest" => crate::audio::commands::InterpolationType::Nearest,
                    "Cubic" => crate::audio::commands::InterpolationType::Cubic,
                    _ => crate::audio::commands::InterpolationType::Linear,
                };
                renderer.set_interpolation(interp);
                let limiter = match limiter_str.as_str() {
                    "SoftKnee" => crate::audio::commands::LimiterMode::SoftKnee,
                    "SoftKneeSmooth" => crate::audio::commands::LimiterMode::SoftKneeSmooth,
                    _ => crate::audio::commands::LimiterMode::HardClip,
                };
                renderer.set_limiter_mode(limiter);
                renderer.set_channels(settings.channel_mode == crate::ui::wav_export_window::ChannelMode::Stereo);

                let render_result = renderer.render_with_settings(
                    &mut writer,
                    &settings,
                    |p| {
                        if cancel_arc.load(Ordering::SeqCst) {
                            return false;
                        }
                        let pct = (p * 100.0) as u32;
                        progress_arc.store(pct, Ordering::SeqCst);
                        state_arc.store(1, Ordering::SeqCst);
                        true
                    },
                );

                match render_result {
                    Ok(()) => {
                        if let Err(e) = writer.finalize() {
                            state_arc.store(3, Ordering::SeqCst);
                            eprintln!("Failed to finalize audio file: {}", e);
                        } else {
                            state_arc.store(2, Ordering::SeqCst);
                            println!("Exported to: {}", file_path.display());
                        }
                    }
                    Err(e) => {
                        state_arc.store(3, Ordering::SeqCst);
                        eprintln!("Failed to render audio: {:?}", e);
                    }
                }
            }
            Err(e) => {
                state_arc.store(3, Ordering::SeqCst);
                eprintln!("Failed to create file: {}", e);
            }
        }
    });
}

pub(crate) fn update_wav_export_progress(app: &mut HtrkApp) {
    if !app.wav_export_state.is_exporting {
        return;
    }

    let progress = app.wav_export_state.export_progress_atomic.load(Ordering::SeqCst);
    let state = app.wav_export_state.export_state_atomic.load(Ordering::SeqCst);

    app.wav_export_state.export_progress = (progress as f32) / 100.0;
    app.wav_export_state.export_status = if progress > 0 {
        format!("Rendering... {}%", progress)
    } else {
        "Preparing...".to_string()
    };

    if state == 2 {
        app.wav_export_state.is_exporting = false;
        app.wav_export_state.export_complete = true;
        app.wav_export_state.export_status = "Export complete!".to_string();
    } else if state == 3 {
        app.wav_export_state.is_exporting = false;
        app.wav_export_state.export_complete = true;
        app.wav_export_state.export_status = "Export failed (check console)".to_string();
    }
}

pub(crate) fn save_config(app: &mut HtrkApp) {
    use crate::ui::file_browser::BrowserMode;

    app.config.last_dirs.clear();
    for (mode, path) in &app.file_browser.last_dirs {
        let key = match mode {
            BrowserMode::Modules => "modules",
            BrowserMode::Samples => "samples",
            BrowserMode::Instruments => "instruments",
            BrowserMode::Projects => "projects",
        };
        app.config.last_dirs.insert(key.to_string(), path.to_string_lossy().into_owned());
    }
    if let Some(ref path) = app.core.file_path {
        app.config.last_file_path = Some(path.clone());
    }
    app.config.favorites = app.file_browser.save_favorites();
    app.file_browser.sync_widths_to_config(&mut app.config);
    app.config.instrument_list_width = Some(app.instrument_editor.list_width);
    app.config.instrument_envelope_height = Some(app.instrument_editor.envelope_height);
    app.config.instrument_envelope_type = Some(app.instrument_editor.envelope_type as u8);
    app.config.instrument_envelope_visible = Some(app.instrument_editor.envelope_visible);
    app.config.sample_list_width = Some(app.sample_editor.list_width);
    app.config.sample_waveform_height = Some(app.sample_editor.waveform_height);
    app.config.save();
}

pub(crate) fn check_auto_backup(app: &mut HtrkApp) {
    let interval = app.config.auto_backup_interval_secs;
    if interval == 0 || !app.core.module_dirty || app.core.module.is_none() {
        return;
    }
    if app.core.last_backup_time.elapsed().as_secs() < interval {
        return;
    }

    let backup_dir = app.config.get_backup_dir();
    let _ = std::fs::create_dir_all(&backup_dir);

    let name = if app.core.loaded_module_name.is_empty() {
        "untitled".to_string()
    } else {
        app.core.loaded_module_name.trim_end_matches(".htk")
            .trim_end_matches(".it")
            .trim_end_matches(".xm")
            .trim_end_matches(".s3m")
            .trim_end_matches(".mod")
            .to_string()
    };
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let backup_path = backup_dir.join(format!("{}_backup_{}.htk", name, timestamp));

    if let Some(ref module) = app.core.module {
        let data = crate::formats::save_module(module);
        let _ = std::fs::write(&backup_path, &data);
    }

    app.core.module_dirty = false;
    app.core.last_backup_time = std::time::Instant::now();
}
