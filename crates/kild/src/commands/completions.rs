use clap::ArgMatches;
use clap_complete::Shell;

use crate::app::build_cli;

pub(crate) fn handle_completions_command(
    matches: &ArgMatches, config: &kild_config::KildConfig
) -> Result<(), Box<dyn std::error::Error>> {
    let shell = matches
        .get_one::<Shell>("shell")
        .ok_or("Shell argument is required")?;

    let mut cmd = build_cli();
    clap_complete::generate(*shell, &mut cmd, "kild", &mut std::io::stdout());

    Ok(())
}
