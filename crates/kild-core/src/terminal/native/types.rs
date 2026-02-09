/// Minimal window info from Core Graphics API.
/// Only contains the fields needed for Ghostty window management.
#[derive(Debug, Clone)]
pub struct NativeWindowInfo {
    /// Core Graphics window ID
    pub id: u32,
    /// Window title
    pub title: String,
    /// Application name
    pub app_name: String,
    /// Process ID (if available)
    pub pid: Option<i32>,
    /// Whether the window is minimized
    pub is_minimized: bool,
}
