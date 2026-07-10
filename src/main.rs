use clap::Parser;
use pyproject_udeps::{Cli, run};
use std::process;
use tracing_log::AsTrace;

fn main() {
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
