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

    /// Set log verbosity (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,
}

fn main() {
    let cli = Cli::parse();

    if cli.daemon {
        eprintln!("Daemon mode is not yet implemented.");
        std::process::exit(1);
    }

    if cli.stdout {
        eprintln!("Pipe mode is not yet implemented.");
        std::process::exit(1);
    }

    if cli.log_level != "info" {
        unsafe { std::env::set_var("ECHOTYPE_LOG", &cli.log_level) };
    }

    echotype_lib::run();
}
