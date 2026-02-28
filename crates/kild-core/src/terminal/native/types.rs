/// Minimal window info from Core Graphics API.
///
/// Internal DTO wrapping data from the CG window enumeration API (via xcap).
/// Fields are public within the crate since this is only constructed in
/// `native::macos` and read in `backends::ghostty`. Validation happens at
/// call sites (e.g., empty title guard in `find_window`, PID availability
/// check in `focus_window`/`minimize_window`).
#[derive(Debug, Clone)]
pub struct NativeWindow {
    /// Core Graphics window ID
    pub id: u32,
    /// Window title
    pub title: String,
    /// Application name
    pub app_name: String,
    /// Process ID (if available). xcap returns u32; converted to i32 for
    /// macOS Accessibility API. `None` if PID unavailable or > i32::MAX.
    pub pid: Option<i32>,
    /// Whether the window is minimized
    pub is_minimized: bool,
}
