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
    let args = Args::parse();

    let mut builder = Builder::from_default_env(); // Initialize from RUST_LOG

    // If --log-level was provided and is not the default, override the filter
    if args.log_level != "info" { // "info" is the default_value in #[arg(default_value = "info")]
        builder.parse_filters(&args.log_level);
    }
    builder.init();

    run_byz_cascading_quorum_v2(args.difficulty);
}
