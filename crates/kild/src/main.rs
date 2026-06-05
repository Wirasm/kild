//! kild CLI — the primary, scriptable interface to kild.
//!
//! Every capability is a subcommand here before it becomes a UI affordance; this
//! is also what an agent drives through the Bash tool (see the kild skill). All
//! orchestration logic lives in `kild-core`; this binary only parses arguments,
//! delegates to a slice, and formats the result (human text, or `--json` for
//! scripts and skills). Reads go to stdout; progress and errors go to stderr, so a
//! caller can parse stdout cleanly. Non-zero exit means failure.

mod commands;

use std::process::ExitCode;

use clap::Parser;

use commands::Cli;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match commands::dispatch(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("\x1b[31merror:\x1b[0m {err:#}");
            ExitCode::FAILURE
        }
    }
}
