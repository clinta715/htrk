/// Holofonic Tracker command-line entry point.
///
/// htrk is a cross-platform tracker with pattern editing, sample
/// editing, CLAP send-bus plugin hosting, automation, and an MCP
/// (Model Context Protocol) server for AI-driven composition.
fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(());
    }
    if args.iter().any(|a| a == "-V" || a == "--version") {
        print_version();
        return Ok(());
    }
    if args.iter().any(|a| a == "--config-path") {
        print_config_path();
        return Ok(());
    }
    if args.iter().any(|a| a == "--list-effects") {
        print_effect_reference();
        return Ok(());
    }
    if args.iter().any(|a| a == "--mcp-help") {
        print_mcp_help();
        return Ok(());
    }
    if args.iter().any(|a| a == "--reset-config") {
        let path = htrk::app_config::AppConfig::config_file();
        if path.exists() {
            std::fs::remove_file(&path).ok();
            eprintln!("Removed config: {}", path.display());
        } else {
            eprintln!("No config to remove at: {}", path.display());
        }
        return Ok(());
    }

    let debug_enabled = args.iter().any(|a| a == "--debug");
    let headless = args.iter().any(|a| a == "--headless");

    let mut config = htrk::app_config::AppConfig::load();
    if debug_enabled || config.debug {
        htrk::debug_log::init(true, htrk::app_config::AppConfig::config_dir());
    }

    // Install a panic hook that appends to <config_dir>/crash.log. Catches
    // panics during eframe shutdown that would otherwise be swallowed.
    htrk::debug_log::install_panic_hook(htrk::app_config::AppConfig::config_dir());

    htrk::debug_log::init_tracing(config.log_file_path.as_deref());

    apply_cli_overrides(&mut config, &args);

    if headless {
        eprintln!("Headless mode: not implemented in this build.");
        eprintln!("(Use --mcp to start the MCP server for scripted access.)");
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([
                config.window_width.unwrap_or(1200.0),
                config.window_height.unwrap_or(800.0),
            ])
            .with_title(format!("Holofonic Tracker v{}", env!("CARGO_PKG_VERSION"))),
        ..Default::default()
    };

    eframe::run_native(
        "htrk",
        options,
        Box::new(|_cc| Ok(Box::new(htrk::app::HtrkApp::with_config(config)))),
    )
}

/// Apply every CLI flag that mutates the loaded AppConfig. Flags
/// without a value (e.g. `--mcp`, `--no-mcp`, `--theme <name>`) are
/// handled here. Errors print a short message and exit non-zero.
fn apply_cli_overrides(config: &mut htrk::app_config::AppConfig, args: &[String]) {
    if args.iter().any(|a| a == "--mcp") {
        config.mcp_enabled = true;
    }
    if args.iter().any(|a| a == "--no-mcp") {
        config.mcp_enabled = false;
    }
    if let Some(pos) = args.iter().position(|a| a == "--mcp-port") {
        match parse_port_arg(args, pos, "--mcp-port") {
            Ok(p) => config.mcp_port = p,
            Err(e) => die(&e),
        }
    }
    if args.iter().any(|a| a == "--mcp-http") {
        config.mcp_http_enabled = true;
    }
    if let Some(pos) = args.iter().position(|a| a == "--mcp-http-port") {
        match parse_port_arg(args, pos, "--mcp-http-port") {
            Ok(p) => config.mcp_http_port = p,
            Err(e) => die(&e),
        }
    }
    if let Some(pos) = args.iter().position(|a| a == "--theme") {
        if let Some(name) = args.get(pos + 1) {
            config.theme_preset = name.clone();
        } else {
            die("--theme requires a preset name (e.g. dark_modern, classic_dos, ...)");
        }
    }
    if let Some(pos) = args.iter().position(|a| a == "--log-file") {
        if let Some(p) = args.get(pos + 1) {
            config.log_file_path = Some(p.clone());
        } else {
            die("--log-file requires a path");
        }
    }
    if let Some(pos) = args.iter().position(|a| a == "--config") {
        if let Some(p) = args.get(pos + 1) {
            eprintln!("note: --config <path> is informational in this build");
            eprintln!("      (the config file location is platform-specific;");
            eprintln!("       run `htrk --config-path` to see the current path)");
            eprintln!("      requested: {}", p);
        } else {
            die("--config requires a path");
        }
    }
}

fn parse_port_arg(args: &[String], pos: usize, flag: &str) -> Result<u16, String> {
    match args.get(pos + 1) {
        Some(s) => s.parse::<u16>().map_err(|_| {
            format!("{} requires a numeric port 0-65535 (got `{}`)", flag, s)
        }),
        None => Err(format!("{} requires a value", flag)),
    }
}

fn die(msg: &str) -> ! {
    eprintln!("error: {}", msg);
    eprintln!("(run `htrk --help` for usage)");
    std::process::exit(1);
}

fn print_version() {
    println!("Holofonic Tracker v{}", env!("CARGO_PKG_VERSION"));
    println!("A cross-platform tracker with CLAP plugin hosting and MCP scripting.");
}

fn print_config_path() {
    let p = htrk::app_config::AppConfig::config_file();
    println!("{}", p.display());
}

fn print_help() {
    let version = env!("CARGO_PKG_VERSION");
    let cfg_path = htrk::app_config::AppConfig::config_file();
    println!("Holofonic Tracker v{}", version);
    println!("A cross-platform tracker with pattern editing, sample editing,");
    println!("CLAP send-bus plugin hosting, automation, and an MCP scripting server.");
    println!();
    println!("USAGE:");
    println!("    htrk [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help                 Print this help and exit");
    println!("    -V, --version              Print version and exit");
    println!("    --config-path              Print the path to the user's config.toml");
    println!("    --list-effects             Print a one-line-per-effect reference");
    println!("    --mcp-help                 Print the MCP server / JSON-RPC reference");
    println!("    --reset-config             Delete the user's config.toml (next launch");
    println!("                                 uses defaults)");
    println!();
    println!("STARTUP OPTIONS:");
    println!("    --debug                    Enable debug log output (also via config.toml)");
    println!("    --theme <NAME>             Override theme preset (dark_modern, classic_dos,");
    println!("                                 amber_terminal, blue_stealth, paper_light,");
    println!("                                 forest_night, hot_pink, deep_purple)");
    println!("    --mcp                      Force-enable the MCP server this session");
    println!("    --no-mcp                   Force-disable the MCP server this session");
    println!("    --mcp-port <N>             Set MCP TCP port (default 18763)");
    println!("    --mcp-http                 Enable MCP HTTP/SSE transport");
    println!("    --mcp-http-port <N>        Set MCP HTTP port (default 18764)");
    println!("    --log-file <PATH>          Write tracing output to a file as well as stderr");
    println!("    --headless                 Reserved (not yet implemented; use --mcp for");
    println!("                                 scripted access without a GUI)");
    println!();
    println!("FILES:");
    println!("    config.toml:    {}", cfg_path.display());
    println!("    On first launch htrk writes a default config.toml with every key commented");
    println!("    out. Edit the file to set default theme, column visibility, MCP ports, etc.");
    println!();
    println!("KEY SHORTCUTS (in-app):");
    println!("    F1                    Show full help (also: Help menu)");
    println!("    F2/F3/F4              Switch to pattern / sample / instrument view");
    println!("    F5                    Play from start of song");
    println!("    F6                    Play pattern from top");
    println!("    F7                    Play from current cursor position");
    println!("    F8                    Stop playback");
    println!("    F10                   Open Settings");
    println!("    Esc                   Toggle edit mode / close dialog");
    println!();
    println!("MORE INFO:");
    println!("    In-app help (F1) has the complete keyboard reference and per-effect");
    println!("    documentation. `htrk --list-effects` prints the 0-F / P-Z-S-R-X quick");
    println!("    reference. `htrk --mcp-help` documents the MCP server's JSON-RPC API.");
    println!();
    println!("EXAMPLES:");
    println!("    htrk                              # launch the GUI with saved config");
    println!("    htrk --mcp --mcp-port 20000       # launch with MCP on a custom port");
    println!("    htrk --theme classic_dos          # launch with a different theme");
    println!("    htrk --debug 2> htrk.log          # log everything to a file");
    println!("    htrk --reset-config               # wipe the config file");
}

/// Quick text-only effect reference. Same data as the in-app help,
/// but readable from a terminal.
fn print_effect_reference() {
    println!("Holofonic Tracker — effect codes (also: F1 in-app)");
    println!();
    println!("  0  Arpeggio            XY: x, y notes (cycle through 3 notes)");
    println!("  1  Portamento Up       XX: speed (0-FF)");
    println!("  2  Portamento Down     XX: speed (0-FF)");
    println!("  3  Tone Portamento     XX: speed (slides pitch toward target note)");
    println!("  4  Vibrato             XY: speed, depth");
    println!("  5  TPort + Vol Slide   XY: speed (high), slide (low)");
    println!("  6  Vibrato + Vol Slide XY: speed (high), slide (low)");
    println!("  7  Tremolo             XY: speed, depth (volume modulation)");
    println!("  8  Set Panning         XX: 00-FF (00=left, 80=center, FF=right)");
    println!("  9  Set Sample Offset   XX: high byte of offset (multiplied by 65536)");
    println!("  A  Volume Slide        XY: up (high), down (low), per tick");
    println!("  B  Position Jump       XX: order index to jump to");
    println!("  C  Set Volume          XX: 00-40 (00=silent, 40=full)");
    println!("  D  Pattern Break       XX: row to break to in next order");
    println!("  E  Extended (E0-EF)    sub-effects: E1 fine porta up, E2 fine down,");
    println!("                         E3 set glissando, E4 set vibrato waveform,");
    println!("                         E5 set finetune, E6 loop pattern, E7 set tremolo");
    println!("                         waveform, E9 retrig note, EA fine vol up, EB fine");
    println!("                         vol down, EC note cut, ED note delay, EE pattern");
    println!("                         delay, EF invert loop");
    println!("  F  Set Speed / Tempo   XX < 20 = ticks/row; XX >= 20 = BPM (XM style)");
    println!();
    println!("Volume column (2-digit decimal, 00-64):");
    println!("  00-40     Set Volume (00=silent, 40=full, same as `C` effect)");
    println!("  41-FF     Volume Slide (up by 0x?0, down by 0x0?)");
    println!("  80-9F     Fine Volume Slide");
    println!("  D0-EF     Panning Slide (D=left, E=right)");
    println!("  F0-FF     Tone Portamento");
    println!();
    println!("HTRK-extended effects (always available):");
    println!("  P  Set Send Bus Param   XY: bus (high), param slot (low), value=volume col");
    println!("  Z  Set Filter Cutoff    XX: 0-FF");
    println!("  S  Set Send Level       XY: bus (high), level 0-FF (low)");
    println!("  R  Set Filter Resonance XX: 0-FF");
    println!("  X  Set Filter Type      XX: 0=off, 1=low, 2=high, 3=band, 4=notch");
    println!();
    println!("See F1 in-app for full details and a per-effect parameter table.");
}

/// MCP / JSON-RPC reference. The MCP server is a sidecar TCP service
/// that exposes the tracker as a tool set: agents can read module
/// state, edit patterns, scan plugin libraries, and play/stop
/// playback without driving the GUI.
fn print_mcp_help() {
    let version = env!("CARGO_PKG_VERSION");
    println!("Holofonic Tracker v{} — MCP (Model Context Protocol) reference", version);
    println!();
    println!("ENABLING THE SERVER");
    println!("    Edit config.toml:");
    println!("        mcp_enabled = true");
    println!("        mcp_port = 18763           # TCP");
    println!("        mcp_http_enabled = true    # optional, for SSE transport");
    println!("        mcp_http_port = 18764");
    println!();
    println!("    Or pass flags on the command line:");
    println!("        htrk --mcp --mcp-port 20000");
    println!();
    println!("    The server starts after the GUI has loaded. It is read-only until");
    println!("    the user (or agent) requests a mutation; mutations are dispatched to");
    println!("    the main thread and execute on the next frame.");
    println!();
    println!("TRANSPORT");
    println!("    Primary: newline-delimited JSON-RPC 2.0 over TCP localhost.");
    println!("        nc localhost 18763        # interactive");
    println!("    Secondary: HTTP/SSE on /mcp (mcp_http_port). Same JSON-RPC 2.0");
    println!("    payloads, framed over Server-Sent Events.");
    println!();
    println!("HANDSHAKE");
    println!("    Send a JSON-RPC initialize request to start a session:");
    println!();
    println!(r#"        {{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{
            "protocolVersion":"2024-11-05",
            "capabilities":{{}},
            "clientInfo":{{"name":"my-agent","version":"1.0"}}
        }}}}"#);
    println!();
    println!("    The server replies with its capabilities (including the tool list).");
    println!("    Call `tools/list` to enumerate tools, `tools/call` to invoke one.");
    println!();
    println!("TOOL CATEGORIES");
    println!();
    println!("    READ-ONLY (server thread, no main-thread lock):");
    println!("        module.get                  Full module JSON (sans sample PCM data)");
    println!("        module.summary              Counts: patterns, samples, instruments");
    println!("        pattern.get                 Read pattern at order/row/channels");
    println!("        playback.status             Current order/row/tick + playing state");
    println!("        channel.state               All channels' volume/pan/mute/solo");
    println!("        plugin.list                 Discovered CLAP plugins");
    println!("        sample_library.list_dir     Browse sample library roots");
    println!("        sample_library.search       Substring search across cached entries");
    println!();
    println!("    MUTATIONS (dispatched to the main thread, then synced to audio):");
    println!("        module.new                  New empty song");
    println!("        module.open                 Open .htk / .xm / .mod / .s3m / .it / .wav");
    println!("        module.save                 Save current module");
    println!("        pattern.set_cell            Set one cell (note/inst/vol/fx)");
    println!("        pattern.bulk_set_cells      Set many cells in one call");
    println!("        pattern.transpose           Transpose selection ±semitones");
    println!("        channel.set_volume          Set channel volume");
    println!("        channel.set_panning         Set channel pan");
    println!("        channel.set_mute            Toggle mute");
    println!("        channel.set_solo            Toggle solo");
    println!("        sample_library.import       Load a WAV from the library into a slot");
    println!("        send_fx.set_plugin          Load / clear a CLAP plugin on a send bus");
    println!("        automation.add_point        Add an automation point");
    println!("        playback.play / stop        Start/stop transport");
    println!();
    println!("EXAMPLE: read the current pattern");
    println!();
    println!(r#"        {{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{
            "name":"pattern.get",
            "arguments":{{
                "order":0,
                "start_row":0,
                "end_row":15,
                "channels":4
            }}
        }}}}"#);
    println!();
    println!("EXAMPLE: set a cell");
    println!();
    println!(r#"        {{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{
            "name":"pattern.set_cell",
            "arguments":{{
                "order":0, "row":0, "channel":0,
                "note":"C-5", "instrument":1, "volume":64,
                "effect":"C", "param":"40"
            }}
        }}}}"#);
    println!();
    println!("NOTE / EFFECT PARSING");
    println!("    Notes: IT/XM style names (C-5, D#4, ---, ===, ^^^, ~~~)");
    println!("           or bare MIDI key numbers (60 = middle C).");
    println!("    Effects: hex string (C02, A04, H83, Z80). Single hex digit is the");
    println!("              command; the two-digit param is its value.");
    println!();
    println!("TROUBLESHOOTING");
    println!("    - `connection refused`: server not started. Check `--mcp` or config.");
    println!("    - `requires mutation dispatch` on stderr: mutation routed OK;");
    println!("      wait one frame and the response will arrive on the same socket.");
    println!("    - 0 plugins listed: the plugin scan is on the main thread on launch.");
    println!("      Wait until the GUI finishes loading, then `tools/call plugin.list`.");
    println!();
    println!("The full schema is exposed by the server itself: send `tools/list`");
    println!("after `initialize` to receive the machine-readable tool manifest.");
}
