use kild_peek_core::window::{MonitorInfo, WindowInfo};

/// Print a formatted table of windows
pub fn print_windows_table(windows: &[WindowInfo]) {
    // Calculate column widths
    let id_width = 6;
    let title_width = windows
        .iter()
        .map(|w| w.title.chars().count())
        .max()
        .unwrap_or(5)
        .clamp(5, 40);
    let app_width = windows
        .iter()
        .map(|w| w.app_name.chars().count())
        .max()
        .unwrap_or(3)
        .clamp(3, 20);
    let size_width = 11; // "1920x1080" format
    let pos_width = 11; // "x:1234 y:1234" format
    let status_width = 9;

    // Header
    println!(
        "┌{}┬{}┬{}┬{}┬{}┬{}┐",
        "─".repeat(id_width + 2),
        "─".repeat(title_width + 2),
        "─".repeat(app_width + 2),
        "─".repeat(size_width + 2),
        "─".repeat(pos_width + 2),
        "─".repeat(status_width + 2),
    );
    println!(
        "│ {:<id_width$} │ {:<title_width$} │ {:<app_width$} │ {:<size_width$} │ {:<pos_width$} │ {:<status_width$} │",
        "ID",
        "Title",
        "App",
        "Size",
        "Position",
        "Status",
        id_width = id_width,
        title_width = title_width,
        app_width = app_width,
        size_width = size_width,
        pos_width = pos_width,
        status_width = status_width,
    );
    println!(
        "├{}┼{}┼{}┼{}┼{}┼{}┤",
        "─".repeat(id_width + 2),
        "─".repeat(title_width + 2),
        "─".repeat(app_width + 2),
        "─".repeat(size_width + 2),
        "─".repeat(pos_width + 2),
        "─".repeat(status_width + 2),
    );

    // Rows
    for window in windows {
        let size = format!("{}x{}", window.width, window.height);
        let pos = format!("x:{} y:{}", window.x, window.y);
        let status = if window.is_minimized {
            "Minimized"
        } else {
            "Visible"
        };

        println!(
            "│ {:<id_width$} │ {:<title_width$} │ {:<app_width$} │ {:<size_width$} │ {:<pos_width$} │ {:<status_width$} │",
            window.id,
            truncate(&window.title, title_width),
            truncate(&window.app_name, app_width),
            truncate(&size, size_width),
            truncate(&pos, pos_width),
            status,
            id_width = id_width,
            title_width = title_width,
            app_width = app_width,
            size_width = size_width,
            pos_width = pos_width,
            status_width = status_width,
        );
    }

    // Footer
    println!(
        "└{}┴{}┴{}┴{}┴{}┴{}┘",
        "─".repeat(id_width + 2),
        "─".repeat(title_width + 2),
        "─".repeat(app_width + 2),
        "─".repeat(size_width + 2),
        "─".repeat(pos_width + 2),
        "─".repeat(status_width + 2),
    );

    println!("\nTotal: {} window(s)", windows.len());
}

/// Print a formatted table of monitors
pub fn print_monitors_table(monitors: &[MonitorInfo]) {
    let id_width = 5;
    let name_width = monitors
        .iter()
        .map(|m| m.name.chars().count())
        .max()
        .unwrap_or(4)
        .clamp(4, 30);
    let size_width = 11;
    let pos_width = 11;
    let primary_width = 7;

    // Header
    println!(
        "┌{}┬{}┬{}┬{}┬{}┐",
        "─".repeat(id_width + 2),
        "─".repeat(name_width + 2),
        "─".repeat(size_width + 2),
        "─".repeat(pos_width + 2),
        "─".repeat(primary_width + 2),
    );
    println!(
        "│ {:<id_width$} │ {:<name_width$} │ {:<size_width$} │ {:<pos_width$} │ {:<primary_width$} │",
        "ID",
        "Name",
        "Size",
        "Position",
        "Primary",
        id_width = id_width,
        name_width = name_width,
        size_width = size_width,
        pos_width = pos_width,
        primary_width = primary_width,
    );
    println!(
        "├{}┼{}┼{}┼{}┼{}┤",
        "─".repeat(id_width + 2),
        "─".repeat(name_width + 2),
        "─".repeat(size_width + 2),
        "─".repeat(pos_width + 2),
        "─".repeat(primary_width + 2),
    );

    // Rows
    for monitor in monitors {
        let size = format!("{}x{}", monitor.width, monitor.height);
        let pos = format!("x:{} y:{}", monitor.x, monitor.y);
        let primary = if monitor.is_primary { "Yes" } else { "No" };

        println!(
            "│ {:<id_width$} │ {:<name_width$} │ {:<size_width$} │ {:<pos_width$} │ {:<primary_width$} │",
            monitor.id,
            truncate(&monitor.name, name_width),
            truncate(&size, size_width),
            truncate(&pos, pos_width),
            primary,
            id_width = id_width,
            name_width = name_width,
            size_width = size_width,
            pos_width = pos_width,
            primary_width = primary_width,
        );
    }

    // Footer
    println!(
        "└{}┴{}┴{}┴{}┴{}┘",
        "─".repeat(id_width + 2),
        "─".repeat(name_width + 2),
        "─".repeat(size_width + 2),
        "─".repeat(pos_width + 2),
        "─".repeat(primary_width + 2),
    );

    println!("\nTotal: {} monitor(s)", monitors.len());
}

/// Truncate a string to a maximum display width, adding "..." if truncated.
///
/// Uses character count (not byte count) to safely handle UTF-8 strings
/// including emoji and multi-byte characters.
pub fn truncate(s: &str, max_len: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_len {
        format!("{:<width$}", s, width = max_len)
    } else {
        // Safely truncate at character boundaries, not byte boundaries
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{:<width$}", format!("{}...", truncated), width = max_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short     ");
        assert_eq!(truncate("this-is-a-very-long-string", 10), "this-is...");
        assert_eq!(truncate("exact", 5), "exact");
    }

    #[test]
    fn test_truncate_edge_cases() {
        assert_eq!(truncate("", 5), "     ");
        assert_eq!(truncate("abc", 3), "abc");
        assert_eq!(truncate("abcd", 3), "...");
    }

    #[test]
    fn test_truncate_utf8_safety() {
        // Emoji are 4 bytes each
        let emoji_text = "Test 🚀 rockets";
        let result = truncate(emoji_text, 10);
        assert_eq!(result.chars().count(), 10);
        assert!(result.ends_with("..."));
    }
}
