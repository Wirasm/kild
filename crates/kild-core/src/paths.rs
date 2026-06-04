//! Centralized filesystem paths for kild's own state.
//!
//! Defaults to `~/.config/kild`, deliberately distinct from the retired
//! kild-old's `~/.kild` so the two tools never share files. Override the whole
//! root with `$KILD_HOME` (useful for tests, sandboxes, or alternate profiles).

use std::path::PathBuf;

/// kild's config/state root: `$KILD_HOME`, else `~/.config/kild`.
///
/// `None` only if neither `$KILD_HOME` nor `$HOME` is set.
pub fn kild_home() -> Option<PathBuf> {
    if let Some(custom) = std::env::var_os("KILD_HOME") {
        return Some(PathBuf::from(custom));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config").join("kild"))
}

/// The project registry file (`<kild_home>/projects.json`).
pub fn projects_file() -> Option<PathBuf> {
    Some(kild_home()?.join("projects.json"))
}

/// Global kild agent definitions (`<kild_home>/agents/`).
pub fn agents_dir() -> Option<PathBuf> {
    Some(kild_home()?.join("agents"))
}

/// Resolved/normalized prompt files passed to pi (`<kild_home>/prompts/`).
pub fn prompts_dir() -> Option<PathBuf> {
    Some(kild_home()?.join("prompts"))
}
