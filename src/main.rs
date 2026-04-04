use clap::Parser;
use std::io::{self, Write};
use ttfautohint_rs::Args;

use ttfautohint_rs::{ttfautohint, InfoData, TtfautohintCall};

fn main() -> io::Result<()> {
    let args = Args::parse();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    if args.ttfa_info {
        // TODO: Implement display_TTFA
        eprintln!("--ttfa-info not yet implemented in Rust frontend");
        std::process::exit(1);
    }

    let output = args.output.clone();
    let call = TtfautohintCall::from_args(&args).unwrap_or_else(|e| {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    });
    let mut idata = InfoData::from_args(&args).unwrap_or_else(|e| {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    });

    let output_bytes = match ttfautohint(&call, &args, &mut idata) {
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
