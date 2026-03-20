use byz_time::run_byz_cascading_quorum_v2;
use clap::Parser;
use env_logger::Builder;
use log::LevelFilter;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Set the difficulty for SHA256 mining (number of leading zeros)
    #[arg(short, long, default_value_t = 3)]
    difficulty: u8,

    /// Set the logging level (off, error, warn, info, debug, trace)
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

fn main() {
    use std::env;

    let args = Args::parse();

    let mut builder = Builder::new();

    // First, set filter based on RUST_LOG environment variable
    builder.parse_filters(&env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()));

    // If --log-level was provided and is not the default, override the filter
    if args.log_level != "info" { // "info" is the default_value in #[arg(default_value = "info")]
        let level = match args.log_level.to_lowercase().as_str() {
            "off" => LevelFilter::Off,
            "error" => LevelFilter::Error,
            "warn" => LevelFilter::Warn,
            "info" => LevelFilter::Info,
            "debug" => LevelFilter::Debug,
            "trace" => LevelFilter::Trace,
            _ => LevelFilter::Info, // Should not happen with clap default_value, but good fallback
        };
        builder.filter_level(level);
    }
    builder.init();

    run_byz_cascading_quorum_v2(args.difficulty);
}
