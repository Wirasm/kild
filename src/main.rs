use shards::{build_cli, init_logging, run_command};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();

    let app = build_cli();
    let matches = app.get_matches();

    run_command(&matches)?;

    Ok(())
}
