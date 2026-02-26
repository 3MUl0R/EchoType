// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clap::Parser;

/// EchoType - Local-first dictation for your desktop
#[derive(Parser, Debug)]
#[command(name = "echotype", version, about)]
struct Cli {
    /// Run in daemon mode (no GUI)
    #[arg(long)]
    daemon: bool,

    /// Pipe mode: output transcription to stdout
    #[arg(long)]
    stdout: bool,

    /// Single-capture mode (use with --stdout)
    #[arg(long)]
    once: bool,

    /// List installed models
    #[arg(long)]
    list_models: bool,

    /// Set the active model by ID
    #[arg(long, value_name = "MODEL_ID")]
    set_model: Option<String>,

    /// Set log verbosity (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,
}

fn main() {
    let cli = Cli::parse();

    if cli.log_level != "info" {
        unsafe { std::env::set_var("ECHOTYPE_LOG", &cli.log_level) };
    }

    if cli.list_models {
        echotype_lib::cli::list_models();
        return;
    }

    if let Some(ref model_id) = cli.set_model {
        echotype_lib::cli::set_model(model_id);
        return;
    }

    if cli.daemon {
        eprintln!("Daemon mode with IPC is planned for a future release.");
        eprintln!("Use --stdout for headless transcription to stdout.");
        std::process::exit(1);
    }

    if cli.stdout {
        echotype_lib::cli::pipe::run(cli.once);
        return;
    }

    echotype_lib::run();
}
