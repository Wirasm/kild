use kild_core::init_logging;

mod app;
mod commands;
mod table;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = app::build_cli();
    let matches = app.get_matches();

    // Extract verbose flag before initializing logging
    // Default (no flag) = quiet mode, -v/--verbose = verbose mode
    let verbose = matches.get_flag("verbose");
    init_logging(!verbose);

    commands::run_command(&matches)?;

    Ok(())
}
