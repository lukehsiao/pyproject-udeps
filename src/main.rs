use clap::Parser;
use pyproject_udeps::{Cli, run};
use std::process;
use tracing_log::AsTrace;

fn main() {
    // The walk parses Python on threads from the ignore crate, which get
    // Rust's 2 MiB default stack. Deeply nested, valid Python overflows
    // that, so ask for the 8 MiB a Linux main thread gets, unless the user
    // chose a size.
    if std::env::var_os("RUST_MIN_STACK").is_none() {
        // SAFETY: No other thread exists yet, so nothing reads the
        // environment while it changes.
        unsafe { std::env::set_var("RUST_MIN_STACK", "8388608") };
    }

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

#[cfg(test)]
mod test {
    use crate::*;
    #[test]
    fn verify_app() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
