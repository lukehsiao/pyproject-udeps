//! The final poetry-udeps release: a thin wrapper around pyproject-udeps,
//! the same tool under its new name.

use clap::Parser;
use pyproject_udeps::{Cli, run};
use std::process;
use tracing_log::AsTrace;

fn main() {
    eprintln!(
        "note: poetry-udeps has been renamed to pyproject-udeps and now also \
         supports uv and PEP 621 projects.\n      install it with: cargo install pyproject-udeps"
    );

    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_max_level(cli.verbose.log_level_filter().as_trace())
        .init();

    match run(&cli) {
        Ok(deps) if deps.is_empty() => process::exit(0),
        Ok(deps) => {
            for dep in deps {
                println!("{dep}");
            }
            process::exit(1);
        }
        Err(e) => {
            eprintln!("{e}");
            process::exit(2)
        }
    }
}
