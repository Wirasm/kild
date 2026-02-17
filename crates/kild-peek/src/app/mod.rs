use clap::{Arg, ArgAction, Command};

mod assert;
mod diff;
mod elements;
mod interact;
mod list;
mod screenshot;

#[cfg(test)]
mod tests;

pub fn build_cli() -> Command {
    Command::new("kild-peek")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Native application inspector for AI-assisted development")
        .long_about(
            "kild-peek provides screenshot capture, window enumeration, and UI state validation \
             for native macOS applications. Designed for AI coding agents that need to see \
             and verify native UI.",
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Enable verbose logging output")
                .action(ArgAction::SetTrue)
                .global(true),
        )
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(list::subcommand())
        .subcommand(screenshot::subcommand())
        .subcommand(diff::subcommand())
        .subcommand(elements::subcommand())
        .subcommand(elements::find_subcommand())
        .subcommand(interact::click_subcommand())
        .subcommand(interact::type_subcommand())
        .subcommand(interact::key_subcommand())
        .subcommand(interact::drag_subcommand())
        .subcommand(interact::scroll_subcommand())
        .subcommand(interact::hover_subcommand())
        .subcommand(assert::subcommand())
}
