use clap::ArgMatches;
use tracing::{info, warn};

use crate::color;

pub(crate) fn handle_init_hooks_command(
    matches: &ArgMatches, config: &kild_config::KildConfig
) -> Result<(), Box<dyn std::error::Error>> {
    let agent = matches
        .get_one::<String>("agent")
        .ok_or("Agent argument is required")?;
    let no_install = matches.get_flag("no-install");

    info!(
        event = "cli.init_hooks_started",
        agent = %agent,
        no_install = no_install
    );

    match agent.as_str() {
        "claude" => init_claude_hooks(),
        "opencode" => init_opencode_hooks(no_install),
        _ => Err(format!("Unsupported agent: {}", agent).into()),
    }
}

fn init_claude_hooks() -> Result<(), Box<dyn std::error::Error>> {
    kild_core::sessions::daemon_helpers::ensure_claude_status_hook().map_err(
        |e| -> Box<dyn std::error::Error> { format!("Failed to create hook script: {}", e).into() },
    )?;
    println!(
        "  {} Installed ~/.kild/hooks/claude-status",
        color::aurora("✓")
    );

    kild_core::sessions::daemon_helpers::ensure_claude_settings().map_err(
        |e| -> Box<dyn std::error::Error> {
            format!("Failed to patch settings.json: {}", e).into()
        },
    )?;
    println!(
        "  {} Configured ~/.claude/settings.json",
        color::aurora("✓")
    );

    println!();
    println!("Claude Code status reporting configured.");
    println!(
        "Agent activity will be reported via {}.",
        color::bold("kild agent-status")
    );

    info!(event = "cli.init_hooks_completed", agent = "claude");

    Ok(())
}

fn init_opencode_hooks(no_install: bool) -> Result<(), Box<dyn std::error::Error>> {
    let cwd = std::env::current_dir()?;

    kild_core::sessions::daemon_helpers::ensure_opencode_plugin_in_worktree(&cwd).map_err(
        |e| -> Box<dyn std::error::Error> { format!("Failed to create plugin file: {}", e).into() },
    )?;
    println!(
        "  {} Created .opencode/plugins/kild-status.ts",
        color::aurora("✓")
    );

    kild_core::sessions::daemon_helpers::ensure_opencode_package_json(&cwd).map_err(
        |e| -> Box<dyn std::error::Error> {
            format!("Failed to create package.json: {}", e).into()
        },
    )?;
    println!("  {} Created .opencode/package.json", color::aurora("✓"));

    kild_core::sessions::daemon_helpers::ensure_opencode_config(&cwd).map_err(
        |e| -> Box<dyn std::error::Error> {
            format!("Failed to patch opencode.json: {}", e).into()
        },
    )?;
    println!("  {} Configured opencode.json", color::aurora("✓"));

    if !no_install {
        let opencode_dir = cwd.join(".opencode");
        let bun_check = std::process::Command::new("bun")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        let bun_available = match &bun_check {
            Ok(s) if s.success() => true,
            Ok(s) => {
                warn!(event = "cli.init_hooks_bun_check_failed", exit_code = ?s.code());
                eprintln!(
                    "  {} bun --version failed (exit {}). Check your bun installation.",
                    color::warning("⚠"),
                    s
                );
                false
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => {
                warn!(event = "cli.init_hooks_bun_check_error", error = %e);
                eprintln!("  {} Error checking bun: {}", color::warning("⚠"), e);
                false
            }
        };

        if bun_available {
            println!(
                "  {} Running {} in .opencode/",
                color::ice("→"),
                color::bold("bun install")
            );
            let status = std::process::Command::new("bun")
                .arg("install")
                .current_dir(&opencode_dir)
                .status();

            match status {
                Ok(s) if s.success() => {
                    println!("  {} Dependencies installed", color::aurora("✓"));
                }
                Ok(s) => {
                    warn!(event = "cli.init_hooks_bun_install_failed", exit_code = ?s.code());
                    eprintln!("  {} bun install exited with {}", color::warning("⚠"), s);
                    eprintln!("  Run `cd .opencode && bun install` manually.");
                }
                Err(e) => {
                    warn!(event = "cli.init_hooks_bun_install_failed", error = %e);
                    eprintln!("  {} Failed to run bun install: {}", color::warning("⚠"), e);
                    eprintln!("  Run `cd .opencode && bun install` manually.");
                }
            }
        } else {
            warn!(event = "cli.init_hooks_bun_not_found");
            eprintln!(
                "  {} bun not found in PATH. Install dependencies manually:",
                color::warning("⚠")
            );
            eprintln!("  cd .opencode && bun install");
        }
    }

    println!();
    println!("OpenCode status reporting configured for this project.");
    println!(
        "Agent activity will be reported via {}.",
        color::bold("kild agent-status")
    );

    info!(event = "cli.init_hooks_completed", agent = "opencode");

    Ok(())
}
