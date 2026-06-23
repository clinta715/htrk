fn print_usage() {
    eprintln!("Holofonic Tracker v{}", std::env!("CARGO_PKG_VERSION"));
    eprintln!("A cross-platform tracker.");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("    htrk [FLAGS]");
    eprintln!();
    eprintln!("FLAGS:");
    eprintln!("    -h, --help           Print this help and exit");
    eprintln!("    -V, --version        Print version and exit");
    eprintln!("    --debug              Enable debug log output");
    eprintln!("    --mcp                Enable the MCP server (overrides config)");
    eprintln!("    --mcp-port <N>       Set MCP TCP port (default: 18763, overrides config)");
    eprintln!("    --mcp-http           Enable MCP HTTP SSE transport (overrides config)");
    eprintln!("    --mcp-http-port <N>  Set MCP HTTP port (default: 18764, overrides config)");
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_usage();
        return Ok(());
    }
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("Holofonic Tracker v{}", std::env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let debug_enabled = args.iter().any(|a| a == "--debug");

    let mut config = htrk::app_config::AppConfig::load();
    if debug_enabled || config.debug {
        htrk::debug_log::init(true, htrk::app_config::AppConfig::config_dir());
    }

    // CLI overrides for MCP
    if args.iter().any(|a| a == "--mcp") {
        config.mcp_enabled = true;
    }
    if let Some(pos) = args.iter().position(|a| a == "--mcp-port") {
        if let Some(port_str) = args.get(pos + 1) {
            if let Ok(port) = port_str.parse::<u16>() {
                config.mcp_port = port;
            } else {
                eprintln!("error: --mcp-port requires a numeric port (0-65535)");
                std::process::exit(1);
            }
        } else {
            eprintln!("error: --mcp-port requires a value");
            std::process::exit(1);
        }
    }
    if args.iter().any(|a| a == "--mcp-http") {
        config.mcp_http_enabled = true;
    }
    if let Some(pos) = args.iter().position(|a| a == "--mcp-http-port") {
        if let Some(port_str) = args.get(pos + 1) {
            if let Ok(port) = port_str.parse::<u16>() {
                config.mcp_http_port = port;
            } else {
                eprintln!("error: --mcp-http-port requires a numeric port (0-65535)");
                std::process::exit(1);
            }
        } else {
            eprintln!("error: --mcp-http-port requires a value");
            std::process::exit(1);
        }
    }

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([config.window_width.unwrap_or(1200.0), config.window_height.unwrap_or(800.0)])
            .with_title("Holofonic Tracker"),
        ..Default::default()
    };

    eframe::run_native(
        "htrk",
        options,
        Box::new(|_cc| Ok(Box::new(htrk::app::HtrkApp::with_config(config)))),
    )
}
