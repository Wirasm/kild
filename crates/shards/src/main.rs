use shards_core::init_logging;

mod app;
mod commands;
mod table;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();

    let app = app::build_cli();
    let matches = app.get_matches();

    commands::run_command(&matches)?;

    Ok(())
}
