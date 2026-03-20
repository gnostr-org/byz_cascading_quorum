use byz_time::run_byz_cascading_quorum_v2;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Set the difficulty for SHA256 mining (number of leading zeros)
    #[arg(short, long, default_value_t = 3)]
    difficulty: u8,
}

fn main() {
    let args = Args::parse();
    run_byz_cascading_quorum_v2(args.difficulty);
}
