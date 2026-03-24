use clap::ArgMatches;
use tracing::{error, info};

pub(crate) fn handle_init_channels_command(
    _matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.init_channels_started");

    let config = kild_core::Config::new();
    let paths = config.paths();

    // 1. Install server.ts and package.json
    kild_core::sessions::daemon_helpers::ensure_channel_server_installed(paths).map_err(|e| {
        error!(event = "cli.init_channels_failed", error = %e);
        Box::<dyn std::error::Error>::from(e)
    })?;

    let fleet_dir = paths.fleet_channel_dir().to_path_buf();
    println!("Channel server installed at {}", fleet_dir.display());

    // 2. Check for bun
    let bun_status = std::process::Command::new("bun")
        .arg("--version")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output();

    match bun_status {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            println!("Bun {version} found");
        }
        _ => {
            eprintln!("Warning: Bun not found. Install from https://bun.sh");
            eprintln!("The fleet channel server requires Bun to run.");
            return Ok(());
        }
    }

    // 3. Install dependencies
    println!("Installing dependencies...");
    let install = std::process::Command::new("bun")
        .args(["install", "--no-summary"])
        .current_dir(&fleet_dir)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| format!("Failed to run bun install: {}", e))?;

    if !install.success() {
        return Err("bun install failed".into());
    }

    println!("\nFleet channel server ready.");
    println!("Enable with: [fleet] channels = true in ~/.kild/config.toml");

    info!(event = "cli.init_channels_completed");
    Ok(())
}
