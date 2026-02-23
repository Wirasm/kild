use kild_core::init_logging;

mod app;
pub(crate) mod color;
mod commands;
mod table;

fn main() {
    let app = app::build_cli();
    let matches = app.get_matches();

    // Handle --no-color before any output
    if matches.get_flag("no-color") {
        color::set_no_color();
    }

    let verbose = matches.get_flag("verbose");
    let quiet = !verbose;
    init_logging(quiet);

    let mut config = match kild_config::KildConfig::load_hierarchy() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(event = "cli.config_load_failed", error = %e);
            kild_config::KildConfig::default()
        }
    };

    // Apply --remote override before any IPC operations.
    if let Some(remote) = matches.get_one::<String>("remote") {
        config.daemon.remote_host = Some(remote.clone());
        if let Some(fingerprint) = matches.get_one::<String>("remote-fingerprint") {
            config.daemon.remote_cert_fingerprint = Some(fingerprint.clone());
        }
    }

    if let Err(e) = commands::run_command(&matches, &config) {
        // Error already printed to user via eprintln! in command handlers.
        // In verbose mode, JSON logs were also emitted.
        // Exit with non-zero code without printing Rust's Debug representation.
        drop(e);
        std::process::exit(1);
    }
}
