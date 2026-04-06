use clap::Parser;
use std::io::{self, Write};
use tilvisan::Args;

use tilvisan::autohint;

fn main() -> io::Result<()> {
    let args = Args::parse();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    if args.ttfa_info {
        // TODO: Implement display_TTFA
        eprintln!("--ttfa-info not yet implemented in Rust frontend");
        std::process::exit(1);
    }

    let output = args.output.clone();

    let output_bytes = match autohint(&args) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1)
        }
    };

    if output == "-" {
        io::stdout().write_all(&output_bytes)?;
    } else {
        std::fs::write(&output, &output_bytes).map_err(|e| {
            eprintln!("Error: Can't write output file '{}': {}", output, e);
            e
        })?;
    }

    Ok(())
}
