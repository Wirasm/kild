use kild_paths::KildPaths;
use tracing::info;

/// Ensure the tmux shim binary is installed at `~/.kild/bin/tmux`.
///
/// Looks for `kild-tmux-shim` next to the running `kild` binary and symlinks
/// it as `tmux` in `~/.kild/bin/`. Agent teams require this binary.
pub(crate) fn ensure_shim_binary() -> Result<(), String> {
    let paths = KildPaths::resolve().map_err(|e| e.to_string())?;
    let shim_bin_dir = paths.bin_dir();
    let shim_link = paths.tmux_shim_binary();

    if shim_link.exists() {
        return Ok(());
    }

    let shim_binary = crate::daemon::find_sibling_binary("kild-tmux-shim")?;

    std::fs::create_dir_all(&shim_bin_dir)
        .map_err(|e| format!("failed to create {}: {}", shim_bin_dir.display(), e))?;

    #[cfg(unix)]
    std::os::unix::fs::symlink(&shim_binary, &shim_link).map_err(|e| {
        format!(
            "failed to symlink {} -> {}: {}",
            shim_binary.display(),
            shim_link.display(),
            e
        )
    })?;

    info!(
        event = "core.session.shim_binary_installed",
        path = %shim_link.display()
    );

    Ok(())
}
