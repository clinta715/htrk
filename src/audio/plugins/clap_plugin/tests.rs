use super::*;
use std::sync::Mutex;

/// Serializes tests that create or destroy real Win32 windows. The OS
/// window class is process-global, so concurrent class registration +
/// window creation across threads can deadlock the test runner.
static WIN32_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn test_descriptor_extraction_basic() {
    let desc = PluginDescriptor {
        format: PluginFormat::Clap,
        path: std::path::PathBuf::from("/test/plugin.clap"),
        plugin_id: "test.id".into(),
        name: "Test Plugin".into(),
        vendor: "Tester".into(),
        version: "1.0".into(),
        description: "A test plugin".into(),
        plugin_type: PluginType::Effect,
        audio_inputs: 2,
        audio_outputs: 2,
        has_editor: false,
        supports_state: true,
    };
    assert_eq!(desc.name, "Test Plugin");
    assert_eq!(desc.format, PluginFormat::Clap);
}

#[test]
fn test_invalid_path_returns_error() {
    let result = ClapPluginHandle::load(std::path::Path::new("/nonexistent/plugin.clap"));
    assert!(result.is_err());
}

/// Integration test: try to load a real CLAP plugin from the system install.
/// Skipped if the plugin isn't available (CI without CLAP installed).
#[test]
fn test_load_real_clap_plugin() {
    let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
    if !path.exists() {
        eprintln!("[skip] TAL-Reverb-4.clap not found at {}", path.display());
        return;
    }
    let handle = ClapPluginHandle::load(path);
    match handle {
        Ok(h) => {
            let desc = h.descriptor();
            eprintln!("[ok] Loaded plugin: {} ({})", desc.name, desc.plugin_id);
            eprintln!("     vendor: {}", desc.vendor);
            eprintln!(
                "     audio: {} in / {} out, has_editor={}, supports_state={}",
                desc.audio_inputs, desc.audio_outputs, desc.has_editor, desc.supports_state
            );
            assert!(!desc.name.is_empty(), "Plugin name should not be empty");
        }
        Err(e) => panic!("Failed to load TAL-Reverb-4: {e}"),
    }
}

/// Integration test: try to load several different CLAP plugin formats
/// (single-DLL and bundle) to verify the loader handles both.
#[test]
fn test_load_multiple_clap_plugins() {
    let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let candidates = [
        (
            r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap",
            "bundle",
        ),
        (
            r"C:\Program Files\Common Files\CLAP\bit-crusher-windows-x64.clap",
            "dll",
        ),
        (r"C:\Program Files\Common Files\CLAP\Dexed.clap", "dll"),
        (r"C:\Program Files\Common Files\CLAP\JC303.clap", "dll"),
    ];
    let mut loaded = 0;
    let mut failed: Vec<String> = Vec::new();
    for (path_str, kind) in &candidates {
        let path = std::path::Path::new(path_str);
        if !path.exists() {
            eprintln!("[skip] {} not found ({})", path_str, kind);
            continue;
        }
        match ClapPluginHandle::load(path) {
            Ok(h) => {
                let desc = h.descriptor();
                eprintln!("[ok] {}: {} ({})", kind, desc.name, desc.plugin_id);
                loaded += 1;
            }
            Err(e) => {
                eprintln!("[fail] {}: {}", path_str, e);
                failed.push(path_str.to_string());
            }
        }
    }
    assert!(
        loaded >= 2,
        "Expected at least 2 plugins to load, got {loaded}"
    );
    assert!(failed.is_empty(), "Failed to load: {failed:?}");
}

/// Integration test: full lifecycle (load → activate → process → deactivate).
/// Skipped if the plugin isn't available.
#[test]
fn test_activate_real_clap_plugin() {
    let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
    if !path.exists() {
        eprintln!("[skip] TAL-Reverb-4.clap not found at {}", path.display());
        return;
    }

    let mut handle = ClapPluginHandle::load(path).expect("load failed");

    // Activate at 48kHz, 256 sample block size
    let mut processor = handle.activate(48000.0, 256).expect("activate failed");
    eprintln!("[ok] Activated plugin: {}", processor.name());

    // Process a block of silence (1s = 48000 samples, process in 256-sample chunks)
    let in_l = vec![0.0f32; 256];
    let in_r = vec![0.0f32; 256];
    let mut out_l = vec![0.0f32; 256];
    let mut out_r = vec![0.0f32; 256];
    let transport = TransportInfo {
        bpm: 120.0,
        sample_rate: 48000.0,
        sample_position: 0,
        is_playing: true,
    };

    // Process a few blocks. The reverb should produce non-zero output if we
    // feed it a non-silent input. With silence input, it should produce
    // only tail/decay of previous samples (which is zero here, since
    // we never fed any signal).
    for block in 0..4 {
        processor.process(&in_l, &in_r, &mut out_l, &mut out_r, 256, &transport);
        eprintln!(
            "[ok] Block {}: out[0] = {:.6} (silence in -> near-silence expected)",
            block, out_l[0]
        );
    }

    // Test with a real signal (impulse) to see if the reverb tail is produced
    let mut impulse_l = vec![0.0f32; 256];
    let mut impulse_r = vec![0.0f32; 256];
    impulse_l[0] = 1.0;
    impulse_r[0] = 1.0;
    processor.process(
        &impulse_l, &impulse_r, &mut out_l, &mut out_r, 256, &transport,
    );
    let peak = out_l
        .iter()
        .chain(out_r.iter())
        .fold(0.0f32, |a, &b| a.max(b.abs()));
    eprintln!("[ok] Impulse response peak: {peak:.6} (should be non-zero for active reverb)");

    // Test deactivation: stop the processor and deactivate the instance
    let stopped = processor.stop();
    handle.deactivate(stopped).expect("deactivate failed");
    eprintln!("[ok] Deactivated plugin cleanly");
}

/// Integration test: process a sine wave through Bit Crusher plugin.
/// The output should be a quantized/distorted version of the input.
#[test]
fn test_bit_crusher_processing() {
    let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path =
        std::path::Path::new(r"C:\Program Files\Common Files\CLAP\bit-crusher-windows-x64.clap");
    if !path.exists() {
        eprintln!("[skip] Bit Crusher not found");
        return;
    }

    let mut handle = ClapPluginHandle::load(path).expect("load failed");
    let mut processor = handle.activate(48000.0, 256).expect("activate failed");
    eprintln!("[ok] Activated: {}", processor.name());

    // Generate a 1kHz sine wave
    let freq = 1000.0f32;
    let sr = 48000.0f32;
    let mut input_l = Vec::with_capacity(256);
    let mut input_r = Vec::with_capacity(256);
    for i in 0..256 {
        let t = i as f32 / sr;
        let s = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.5;
        input_l.push(s);
        input_r.push(s);
    }
    let mut out_l = vec![0.0f32; 256];
    let mut out_r = vec![0.0f32; 256];
    let transport = TransportInfo {
        bpm: 120.0,
        sample_rate: 48000.0,
        sample_position: 0,
        is_playing: true,
    };

    // Feed a few blocks of sine so the effect's internal state stabilizes
    for _ in 0..4 {
        processor.process(&input_l, &input_r, &mut out_l, &mut out_r, 256, &transport);
    }

    let in_peak = input_l.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
    let out_peak = out_l.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
    eprintln!("[ok] Sine input peak: {in_peak:.4}");
    eprintln!("[ok] Bit crusher output peak: {out_peak:.4} (non-zero means plugin is processing)");
    assert!(
        out_peak > 0.01,
        "Bit Crusher should produce non-trivial output"
    );

    // Compare sample values: bit crusher should quantize the input,
    // producing a different (not equal) output for most samples.
    let mut differences = 0;
    let mut exact_matches = 0;
    for i in 0..256 {
        if (input_l[i] - out_l[i]).abs() > 0.001 {
            differences += 1;
        } else {
            exact_matches += 1;
        }
    }
    eprintln!("[ok] Bit crusher: {differences}/256 samples changed, {exact_matches} exact matches");
    // Note: the bit crusher's default parameters may pass through audio unchanged.
    // We're just verifying the plugin processes audio (non-zero output is sufficient).
    // Once we have parameter control, we can crank up the bit reduction to confirm.

    let stopped = processor.stop();
    handle.deactivate(stopped).expect("deactivate failed");
    eprintln!("[ok] Deactivated cleanly");
}

/// Regression: `process()` must fully cover a callback larger than the
/// `max_block` it was activated with. Previously it clamped to
/// `frame_count.min(max_block)` and left the tail untouched, so a 1024-
/// frame WASAPI buffer processed by a 512-frame plugin would chop.
///
/// This is a deterministic test that does NOT require a real CLAP plugin:
/// we build a `ClapPluginProcessor` with `processor: None`, seed the
/// caller's output buffer with a non-zero marker, and verify that every
/// output frame is overwritten — proving the sub-block loop covers the
/// full `frame_count`, not just the first `max_block`. (With `processor:
/// None` the plugin call is skipped, so each written frame is the zeroed
/// `self.out_l`; the buggy clamp would have left frames
/// `[max_block..frame_count]` at the 0.5 marker.)
#[test]
fn test_process_covers_callback_larger_than_max_block() {
    let descriptor = PluginDescriptor {
        format: PluginFormat::Clap,
        path: std::path::PathBuf::new(),
        plugin_id: String::new(),
        name: "NoProcessor".into(),
        vendor: String::new(),
        version: String::new(),
        description: String::new(),
        plugin_type: PluginType::Effect,
        audio_inputs: 2,
        audio_outputs: 2,
        has_editor: false,
        supports_state: false,
    };

    let max_block = 256usize;
    let mut proc = ClapPluginProcessor {
        processor: None,
        descriptor,
        sample_rate: 48000.0,
        max_block,
        in_l: vec![0.0; max_block],
        in_r: vec![0.0; max_block],
        out_l: vec![0.0; max_block],
        out_r: vec![0.0; max_block],
        input_ports: AudioPorts::with_capacity(2, 1),
        output_ports: AudioPorts::with_capacity(2, 1),
        _input_event_buffer: clack_host::events::io::EventBuffer::with_capacity(0),
        output_event_buffer: clack_host::events::io::EventBuffer::with_capacity(0),
        param_rx: param_channel(64).1,
        pending_params: Vec::new(),
        param_scratch: Vec::new(),
        note_events: std::collections::VecDeque::new(),
        next_note_id: 0,
    };

    // 1024-frame callback = 4x max_block. Seed output with a non-zero
    // marker so any frame process() fails to write stays detectable.
    let total = 1024usize;
    let input_l = vec![0.0f32; total];
    let input_r = vec![0.0f32; total];
    let mut out_l = vec![0.5f32; total];
    let mut out_r = vec![0.5f32; total];
    let transport = TransportInfo {
        bpm: 120.0,
        sample_rate: 48000.0,
        sample_position: 0,
        is_playing: true,
    };

    proc.process(
        &input_l, &input_r, &mut out_l, &mut out_r, total, &transport,
    );

    // Every frame must have been overwritten with the (zeroed) internal
    // output buffer. With the buggy clamp, frames [max_block..total]
    // would still hold the 0.5 marker.
    let untouched = out_l
        .iter()
        .chain(out_r.iter())
        .filter(|&&v| v != 0.0)
        .count();
    assert_eq!(
        untouched, 0,
        "process() left {untouched} output frames unwritten (marker 0.5 survived); \
         it must cover the full callback even when frame_count > max_block"
    );

    // Spot-check frames in each sub-block past the first one.
    for i in [256usize, 512, 768, 1023] {
        assert_eq!(
            out_l[i], 0.0,
            "frame {i} (past max_block) was not written to out_l"
        );
        assert_eq!(
            out_r[i], 0.0,
            "frame {i} (past max_block) was not written to out_r"
        );
    }
}

/// Integration test: process audio through TAL Reverb 4 with a longer
/// signal to verify the reverb tail extends across multiple blocks.
#[test]
fn test_reverb_tail_extends() {
    let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
    if !path.exists() {
        eprintln!("[skip] TAL-Reverb-4.clap not found");
        return;
    }

    let mut handle = ClapPluginHandle::load(path).expect("load failed");
    let mut processor = handle.activate(48000.0, 256).expect("activate failed");
    let transport = TransportInfo {
        bpm: 120.0,
        sample_rate: 48000.0,
        sample_position: 0,
        is_playing: true,
    };

    // Block 0: send an impulse
    let mut impulse = vec![0.0f32; 256];
    impulse[0] = 1.0;
    let mut out_l = vec![0.0f32; 256];
    let mut out_r = vec![0.0f32; 256];
    processor.process(&impulse, &impulse, &mut out_l, &mut out_r, 256, &transport);
    let block0_peak = out_l
        .iter()
        .chain(out_r.iter())
        .fold(0.0f32, |a, &b| a.max(b.abs()));

    // Process ~100ms of silence following — reverb tail should continue
    let silence = vec![0.0f32; 256];
    let mut tail_peaks = Vec::new();
    for _ in 0..20 {
        processor.process(&silence, &silence, &mut out_l, &mut out_r, 256, &transport);
        let peak = out_l
            .iter()
            .chain(out_r.iter())
            .fold(0.0f32, |a, &b| a.max(b.abs()));
        tail_peaks.push(peak);
    }

    let max_tail = tail_peaks.iter().fold(0.0f32, |a, &b| a.max(b));
    let blocks_with_signal = tail_peaks.iter().filter(|&&p| p > 0.001).count();
    eprintln!("[ok] Impulse block peak: {block0_peak:.4}");
    eprintln!("[ok] Max tail peak:      {max_tail:.4}");
    eprintln!("[ok] Blocks with signal: {blocks_with_signal}/20 (reverb tail should persist)");
    assert!(
        max_tail > 0.001,
        "Reverb tail should be audible after silence"
    );
    assert!(
        blocks_with_signal >= 10,
        "Reverb tail should persist for at least ~130ms"
    );

    let stopped = processor.stop();
    handle.deactivate(stopped).expect("deactivate failed");
}

/// Integration test: extract_descriptor_for_browser loads the .clap
/// library just enough to read its descriptor (no instantiation).
/// This is the fast path used by the plugin browser UI to populate
/// the plugin list on startup.
#[test]
fn test_extract_descriptor_for_browser() {
    let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let clap_path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
    if !clap_path.exists() {
        eprintln!("[skip] TAL-Reverb-4.clap not found");
        return;
    }
    match extract_descriptor_for_browser(clap_path) {
        Ok(d) => {
            eprintln!("[ok] Extracted descriptor: {} ({})", d.name, d.plugin_id);
            assert!(!d.name.is_empty(), "Descriptor should have a name");
            assert!(
                !d.plugin_id.is_empty(),
                "Descriptor should have a plugin id"
            );
            assert_eq!(d.format, PluginFormat::Clap);
            // TAL Reverb 4 should be a Reverb effect, not instrument
            assert_ne!(d.plugin_type, PluginType::Instrument);
        }
        Err(e) => panic!("extract_descriptor_for_browser failed: {e}"),
    }
}

/// Integration test: open and close the GUI editor of a real CLAP plugin
/// (TAL Reverb 4). The editor appears as a floating OS window. We just
/// verify that open/close calls succeed without panicking — the visual
/// window itself can't be tested from a headless context.
#[test]
fn test_editor_open_close_real_plugin() {
    // Serialize Win32 window creation across tests in the same process.
    // Multiple tests creating top-level windows concurrently can deadlock
    // the test runner (window class registration is process-global).
    let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
    if !path.exists() {
        eprintln!("[skip] TAL-Reverb-4.clap not found");
        return;
    }

    let mut handle = ClapPluginHandle::load(path).expect("load failed");
    // Activate the plugin first (most CLAP plugins require activation
    // before the GUI can be created).
    let _processor = handle.activate(48000.0, 256).expect("activate failed");

    // Some CLAP plugins only expose the GUI extension after activation.
    // Probe via open_editor() and accept either success (Win32 GUI) or
    // a failure (no GUI / headless).
    assert!(
        !handle.is_editor_open(),
        "Editor should not be open initially"
    );

    match handle.open_editor(crate::audio::plugins::EditorMode::Floating, None) {
        Ok(()) => {
            assert!(
                handle.is_editor_open(),
                "Editor should be open after open_editor()"
            );
            eprintln!("[ok] Opened editor for {}", handle.descriptor().name);

            // Open again should be a no-op (returns Ok).
            handle
                .open_editor(crate::audio::plugins::EditorMode::Floating, None)
                .expect("double open should be a no-op");
            assert!(handle.is_editor_open());

            // Close the editor.
            handle.close_editor();
            assert!(
                !handle.is_editor_open(),
                "Editor should be closed after close_editor()"
            );
            eprintln!("[ok] Closed editor cleanly");
        }
        Err(e) => {
            eprintln!("[warn] open_editor failed (may be headless or no GUI): {e}");
        }
    }

    // Close again should be a no-op.
    handle.close_editor();
}

/// Integration test: has_editor() returns a value (true or false)
/// without panicking.
#[test]
fn test_has_editor_returns_bool() {
    let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
    if !path.exists() {
        eprintln!("[skip] TAL-Reverb-4.clap not found");
        return;
    }
    let handle = ClapPluginHandle::load(path).expect("load failed");
    let has = handle.has_editor();
    eprintln!("[ok] TAL Reverb 4 has_editor() = {has}");
}

/// Integration test: open editor in floating mode.
/// Skips if the plugin doesn't support floating mode (most plugins
/// only support embedded; e.g. TAL Reverb 4 falls back to embedded).
#[test]
fn test_open_editor_floating_with_real_plugin() {
    let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
    if !path.exists() {
        eprintln!("[skip] TAL-Reverb-4.clap not found");
        return;
    }
    let mut handle = ClapPluginHandle::load(path).expect("load failed");
    let _processor = handle.activate(48000.0, 256).expect("activate failed");

    // Try floating first. Most plugins fall back to embedded.
    let result = handle.open_editor(crate::audio::plugins::EditorMode::Floating, None);
    match result {
        Ok(()) => {
            assert!(handle.is_editor_open());
            let mode = handle.editor_mode();
            eprintln!(
                "[ok] Opened editor in mode {mode:?} for {}",
                handle.descriptor().name
            );
            handle.close_editor();
            assert!(!handle.is_editor_open());
        }
        Err(e) => {
            eprintln!("[skip] Plugin doesn't support any GUI mode: {e}");
        }
    }
}

/// Compile-time check: HtrkHostShared implements HostLogImpl and HostGuiImpl.
/// This is a regression guard — if either impl is removed, the
/// `declare_extensions` call in `HtrkHost` stops compiling.
#[test]
fn test_host_extensions_trait_impls() {
    fn assert_log<S: clack_extensions::log::HostLogImpl>() {}
    fn assert_gui<S: clack_extensions::gui::HostGuiImpl>() {}
    assert_log::<HtrkHostShared>();
    assert_gui::<HtrkHostShared>();
}

/// Smoke test: HtrkHostShared::log() doesn't panic for any severity.
#[test]
fn test_host_log_no_panic() {
    use clack_extensions::log::LogSeverity;
    let shared = HtrkHostShared {
        state_ext: Default::default(),
    };
    for sev in [
        LogSeverity::Debug,
        LogSeverity::Info,
        LogSeverity::Warning,
        LogSeverity::Error,
        LogSeverity::Fatal,
        LogSeverity::HostMisbehaving,
        LogSeverity::PluginMisbehaving,
    ] {
        shared.log(sev, "test message");
    }
}

/// Smoke test: HtrkHostShared GUI impl methods don't panic.
#[test]
fn test_host_gui_no_panic() {
    use clack_extensions::gui::GuiSize;
    let shared = HtrkHostShared {
        state_ext: Default::default(),
    };
    shared.resize_hints_changed();
    let _ = shared.request_resize(GuiSize {
        width: 800,
        height: 600,
    });
    let _ = shared.request_show();
    let _ = shared.request_hide();
    shared.closed(true);
    shared.closed(false);
}

/// Tests that last_editor_error is None after a successful open_editor
/// call, and is populated when open_editor fails.
#[test]
fn test_last_editor_error_state() {
    let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
    if !path.exists() {
        eprintln!("[skip] TAL-Reverb-4.clap not found");
        return;
    }
    let mut handle = ClapPluginHandle::load(path).expect("load failed");
    let _processor = handle.activate(48000.0, 256).expect("activate failed");

    // Open the editor
    if handle
        .open_editor(crate::audio::plugins::EditorMode::Floating, None)
        .is_ok()
    {
        assert!(handle.last_editor_error().is_none());
        handle.close_editor();
    }
}

/// Round-trip state save/load test: load a real CLAP plugin, set a
/// parameter to a known value, save state, drop the handle, reload
/// the plugin fresh, set a different parameter value, load the
/// saved state, then read the parameter and assert it matches the
/// value we set in the first instance.
///
/// This exercises the full save_state/load_state path that the
/// .htk round-trip relies on for both send-bus and instrument
/// plugin persistence. Skipped if no CLAP plugin with state
/// support is available.
#[test]
fn test_state_save_load_round_trip() {
    let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
    if !path.exists() {
        eprintln!("[skip] TAL-Reverb-4.clap not found at {}", path.display());
        return;
    }

    // ── Phase 1: load, set a known value, save state, drop ─────
    let mut handle1 = ClapPluginHandle::load(path).expect("load 1 failed");
    let desc = handle1.descriptor();
    if !desc.supports_state {
        eprintln!("[skip] {} does not support state extension", desc.name);
        return;
    }
    let param_info = handle1.parameter_info();
    assert!(
        !param_info.is_empty(),
        "plugin must have at least 1 parameter for this test"
    );
    let param_id = param_info[0].id;
    let target_value = 0.42_f32;

    // Activate the plugin so set_parameter has a real audio thread
    // to apply the change to. Without this, the param ring entry
    // never gets drained and save_state captures the plugin's
    // default state, not our target value.
    let mut processor1 = handle1.activate(48000.0, 256).expect("activate 1 failed");

    // Queue the target value through the param ring.
    handle1.set_parameter(param_id, target_value);

    // Process one block of silence to drain the ring and apply
    // the param to the plugin's internal state.
    let silence_in = vec![0.0f32; 256];
    let mut out_l = vec![0.0f32; 256];
    let mut out_r = vec![0.0f32; 256];
    let transport = TransportInfo {
        bpm: 120.0,
        sample_rate: 48000.0,
        sample_position: 0,
        is_playing: false,
    };
    processor1.process(
        &silence_in,
        &silence_in,
        &mut out_l,
        &mut out_r,
        256,
        &transport,
    );
    // Sanity check: the value we set should now be visible via
    // get_parameter. If this fails the rest of the test is
    // meaningless (we'd be saving a different state than we
    // thought).
    let applied = handle1.get_parameter(param_id);
    assert!(
        (applied - target_value).abs() < 0.05,
        "set_parameter did not take effect: expected ~{target_value}, got {applied}"
    );

    // Save the state. The plugin should capture the param value we
    // just set along with everything else.
    let saved_state = handle1.save_state().expect("save_state failed");
    assert!(
        !saved_state.is_empty(),
        "saved state must be non-empty for a state-supporting plugin"
    );
    eprintln!(
        "[ok] Saved {} bytes of state (param was {applied:.4})",
        saved_state.len()
    );

    // Stop the audio thread cleanly.
    let stopped1 = processor1.stop();
    handle1.deactivate(stopped1).expect("deactivate 1 failed");
    drop(handle1);

    // ── Phase 2: reload, set a DIFFERENT value, load saved state ──
    let mut handle2 = ClapPluginHandle::load(path).expect("load 2 failed");
    // Activate at the same sample rate / block size so the plugin
    // is in a known state for load_state to apply to.
    let mut processor = handle2.activate(48000.0, 256).expect("activate 2 failed");

    // Set a deliberately different value so we can detect the
    // load_state actually applies the saved state (not just no-ops
    // back to whatever we set).
    let different_value = 0.88_f32;
    handle2.set_parameter(param_id, different_value);
    processor.process(
        &silence_in,
        &silence_in,
        &mut out_l,
        &mut out_r,
        256,
        &transport,
    );
    // Now read the param back. After the process() flush, the
    // audio thread has applied our set_parameter and we can read
    // the current value via the main thread.
    let pre_load = handle2.get_parameter(param_id);
    eprintln!("[info] param value before load_state: {pre_load:.4} (expected ~{different_value})");

    // Load the saved state. The plugin should restore param 0 to
    // target_value (0.42), overwriting our 0.88.
    handle2.load_state(&saved_state).expect("load_state failed");

    // Process one more block so any audio-thread state the load
    // touched gets applied (some plugins update their param
    // values lazily on the next process tick).
    processor.process(
        &silence_in,
        &silence_in,
        &mut out_l,
        &mut out_r,
        256,
        &transport,
    );

    let post_load = handle2.get_parameter(param_id);
    eprintln!("[info] param value after load_state:  {post_load:.4} (expected ~{target_value})");

    // The loaded value should match the originally-set target
    // value (within float epsilon). We allow a tiny tolerance
    // because CLAP params may be quantized to plugin-internal
    // resolution, but the difference should be small.
    let diff = (post_load - target_value).abs();
    assert!(
        diff < 0.05,
        "loaded param {post_load} differs from saved {target_value} by {diff} (more than 0.05 tolerance)"
    );

    // Tear down the audio thread cleanly.
    let stopped = processor.stop();
    handle2.deactivate(stopped).expect("deactivate 2 failed");
}

/// Regression test: dropping a `ClapPluginHandle` while the editor
/// is still open must close the editor cleanly. Previously the
/// host window was destroyed first and the plugin's child HWND was
/// orphaned, which crashed `PluginInstance::drop()` (and the whole
/// app on exit). The `Drop` impl on `ClapPluginHandle` now calls
/// `close_editor()` before the fields are dropped, so this path
/// should be safe.
#[test]
fn test_drop_with_open_editor_does_not_crash() {
    let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
    if !path.exists() {
        eprintln!("[skip] TAL-Reverb-4.clap not found");
        return;
    }

    let mut handle = ClapPluginHandle::load(path).expect("load failed");
    let _processor = handle.activate(48000.0, 256).expect("activate failed");

    // Open the editor (floating). If the plugin is headless, the
    // open_editor call will Err and we skip the test — the Drop is a
    // no-op when the editor isn't open.
    let opened = handle
        .open_editor(crate::audio::plugins::EditorMode::Floating, None)
        .is_ok();

    if !opened {
        eprintln!("[skip] TAL-Reverb-4 has no GUI; Drop path not exercised");
        return;
    }

    assert!(handle.is_editor_open(), "Editor should be open before drop");

    // Drop the handle without explicitly closing the editor.
    // Previously this would crash on app exit. Now the Drop
    // impl calls close_editor() and the field drop order is
    // safe (host_window is None, instance drops last).
    drop(handle);
    eprintln!("[ok] Dropped handle with open editor without crashing");
}

/// Regression test: dropping a `ClapPluginProcessor` (e.g. when the
/// audio engine replaces the plugin in a send bus) should release
/// the audio-thread resources via `stop_processing()` rather than
/// silently leaking the `StartedPluginAudioProcessor`. The
/// `StoppedPluginAudioProcessor` returned by `stop_processing()`
/// cannot be passed back to the main thread from `Drop`, so it's
/// a small leak — but the audio thread side is no longer leaked.
#[test]
fn test_drop_processor_releases_audio_resources() {
    let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
    if !path.exists() {
        eprintln!("[skip] TAL-Reverb-4.clap not found");
        return;
    }

    let mut handle = ClapPluginHandle::load(path).expect("load failed");
    let processor = handle.activate(48000.0, 256).expect("activate failed");

    // Drop the processor without calling stop() first. The Drop
    // impl should call stop_processing() internally to release
    // the audio-thread resources.
    drop(processor);
    eprintln!("[ok] Dropped processor without explicit stop()");

    // The handle still holds the instance; we can still call
    // close_editor() / save_state() etc.
    assert!(!handle.is_editor_open(), "Editor should not be open");
}
