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

    let level = match args.log_level.to_lowercase().as_str() {
        "off" => LevelFilter::Off,
        "error" => LevelFilter::Error,
        "warn" => LevelFilter::Warn,
        "info" => LevelFilter::Info,
        "debug" => LevelFilter::Debug,
        "trace" => LevelFilter::Trace,
        _ => LevelFilter::Info,
    };

    Builder::new().filter_level(level).init();

    run_byz_cascading_quorum_v2(args.difficulty);
}
