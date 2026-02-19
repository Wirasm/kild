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

    // Apply --remote override before any IPC operations.
    if let Some(remote) = matches.get_one::<String>("remote") {
        let fingerprint = matches
            .get_one::<String>("remote-fingerprint")
            .map(|s| s.as_str());
        kild_core::daemon::set_remote_override(remote, fingerprint);
    }

    if let Err(e) = commands::run_command(&matches) {
        // Error already printed to user via eprintln! in command handlers.
        // In verbose mode, JSON logs were also emitted.
        // Exit with non-zero code without printing Rust's Debug representation.
        drop(e);
        std::process::exit(1);
    }
}
