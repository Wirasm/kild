use kild_peek_core::init_logging;

mod app;
mod commands;
mod table;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = app::build_cli();
    let matches = app.get_matches();

    // Extract quiet flag before initializing logging
    let quiet = matches.get_flag("quiet");
    init_logging(quiet);

    commands::run_command(&matches)?;

    Ok(())
}
