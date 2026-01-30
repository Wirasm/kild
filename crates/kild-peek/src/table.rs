use kild_peek_core::element::ElementInfo;
use kild_peek_core::window::{MonitorInfo, WindowInfo};

/// Print a formatted table of windows
pub fn print_windows_table(windows: &[WindowInfo]) {
    // Calculate column widths
    let id_width = 6;
    let title_width = windows
        .iter()
        .map(|w| w.title().chars().count())
        .max()
        .unwrap_or(5)
        .clamp(5, 40);
    let app_width = windows
        .iter()
        .map(|w| w.app_name().chars().count())
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
        let size = format!("{}x{}", window.width(), window.height());
        let pos = format!("x:{} y:{}", window.x(), window.y());
        let status = if window.is_minimized() {
            "Minimized"
        } else {
            "Visible"
        };

        println!(
            "│ {:<id_width$} │ {:<title_width$} │ {:<app_width$} │ {:<size_width$} │ {:<pos_width$} │ {:<status_width$} │",
            window.id(),
            truncate(window.title(), title_width),
            truncate(window.app_name(), app_width),
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
        .map(|m| m.name().chars().count())
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
        let size = format!("{}x{}", monitor.width(), monitor.height());
        let pos = format!("x:{} y:{}", monitor.x(), monitor.y());
        let primary = if monitor.is_primary() { "Yes" } else { "No" };

        println!(
            "│ {:<id_width$} │ {:<name_width$} │ {:<size_width$} │ {:<pos_width$} │ {:<primary_width$} │",
            monitor.id(),
            truncate(monitor.name(), name_width),
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

/// Print a formatted table of UI elements
pub fn print_elements_table(elements: &[ElementInfo]) {
    let role_width = elements
        .iter()
        .map(|e| e.role().chars().count())
        .max()
        .unwrap_or(4)
        .clamp(4, 20);
    let title_width = elements
        .iter()
        .filter_map(|e| e.title())
        .map(|t| t.chars().count())
        .max()
        .unwrap_or(5)
        .clamp(5, 30);
    let value_width = elements
        .iter()
        .filter_map(|e| e.value())
        .map(|v| v.chars().count())
        .max()
        .unwrap_or(5)
        .clamp(5, 30);
    let pos_width = 11;
    let size_width = 11;
    let enabled_width = 7;

    // Header
    println!(
        "┌{}┬{}┬{}┬{}┬{}┬{}┐",
        "─".repeat(role_width + 2),
        "─".repeat(title_width + 2),
        "─".repeat(value_width + 2),
        "─".repeat(pos_width + 2),
        "─".repeat(size_width + 2),
        "─".repeat(enabled_width + 2),
    );
    println!(
        "│ {:<role_width$} │ {:<title_width$} │ {:<value_width$} │ {:<pos_width$} │ {:<size_width$} │ {:<enabled_width$} │",
        "Role",
        "Title",
        "Value",
        "Position",
        "Size",
        "Enabled",
        role_width = role_width,
        title_width = title_width,
        value_width = value_width,
        pos_width = pos_width,
        size_width = size_width,
        enabled_width = enabled_width,
    );
    println!(
        "├{}┼{}┼{}┼{}┼{}┼{}┤",
        "─".repeat(role_width + 2),
        "─".repeat(title_width + 2),
        "─".repeat(value_width + 2),
        "─".repeat(pos_width + 2),
        "─".repeat(size_width + 2),
        "─".repeat(enabled_width + 2),
    );

    // Rows
    for elem in elements {
        let title = elem.title().unwrap_or("-");
        let value = elem.value().unwrap_or("-");
        let pos = format!("x:{} y:{}", elem.x(), elem.y());
        let size = format!("{}x{}", elem.width(), elem.height());
        let enabled = if elem.enabled() { "Yes" } else { "No" };

        println!(
            "│ {:<role_width$} │ {:<title_width$} │ {:<value_width$} │ {:<pos_width$} │ {:<size_width$} │ {:<enabled_width$} │",
            truncate(elem.role(), role_width),
            truncate(title, title_width),
            truncate(value, value_width),
            truncate(&pos, pos_width),
            truncate(&size, size_width),
            enabled,
            role_width = role_width,
            title_width = title_width,
            value_width = value_width,
            pos_width = pos_width,
            size_width = size_width,
            enabled_width = enabled_width,
        );
    }

    // Footer
    println!(
        "└{}┴{}┴{}┴{}┴{}┴{}┘",
        "─".repeat(role_width + 2),
        "─".repeat(title_width + 2),
        "─".repeat(value_width + 2),
        "─".repeat(pos_width + 2),
        "─".repeat(size_width + 2),
        "─".repeat(enabled_width + 2),
    );

    println!("\nTotal: {} element(s)", elements.len());
}

/// Print elements as an indented tree hierarchy using box-drawing characters.
///
/// Elements are expected in depth-first traversal order (as returned by the
/// Accessibility API). Uses a look-ahead approach—scanning forward from each
/// element—to determine sibling relationships without parent tracking or tree
/// reconstruction from the flat list (`└──` vs `├──`).
pub fn print_elements_tree(elements: &[ElementInfo]) {
    if elements.is_empty() {
        return;
    }

    for (i, elem) in elements.iter().enumerate() {
        let depth = elem.depth();

        if depth == 0 {
            // Root-level element: no connector
            print_tree_node(elem, "");
        } else {
            // Determine connector: └── if last sibling, ├── otherwise
            let is_last = is_last_sibling(elements, i, depth);
            let connector = if is_last { "└── " } else { "├── " };

            // Build indent prefix from ancestor levels
            let indent = build_tree_indent(elements, i, depth);

            print_tree_node(elem, &format!("{}{}", indent, connector));
        }
    }

    println!("\nTotal: {} element(s)", elements.len());
}

/// Check if element at `index` is the last sibling at the given `depth`.
///
/// A sibling is the next element at the same depth that shares the same parent.
/// We scan forward until we find an element at the same or lesser depth.
fn is_last_sibling(elements: &[ElementInfo], index: usize, depth: usize) -> bool {
    elements[index + 1..]
        .iter()
        .find(|elem| elem.depth() <= depth)
        .is_none_or(|elem| elem.depth() < depth)
}

/// Build the indent prefix for a tree node at the given depth.
///
/// For each ancestor level (1..depth), we check whether that ancestor still
/// has siblings coming. If yes, draw "│   "; if no, draw "    ".
fn build_tree_indent(elements: &[ElementInfo], index: usize, depth: usize) -> String {
    let mut indent = String::new();
    for level in 1..depth {
        // Check if there's a future element at this ancestor level
        let ancestor_has_more = has_future_sibling_at_level(elements, index, level);
        if ancestor_has_more {
            indent.push_str("│   ");
        } else {
            indent.push_str("    ");
        }
    }
    indent
}

/// Check if there's a future element at the given level after the current index.
fn has_future_sibling_at_level(elements: &[ElementInfo], index: usize, level: usize) -> bool {
    elements[index + 1..]
        .iter()
        .find(|elem| elem.depth() <= level)
        .is_some_and(|elem| elem.depth() == level)
}

/// Print a single tree node with the given prefix.
fn print_tree_node(elem: &ElementInfo, prefix: &str) {
    let mut line = format!("{}{}", prefix, elem.role());

    // Show title if present
    if let Some(title) = elem.title() {
        line.push_str(&format!(" \"{}\"", title));
    }

    // Show size if non-zero
    if elem.width() > 0 || elem.height() > 0 {
        line.push_str(&format!(" [{}x{}]", elem.width(), elem.height()));
    }

    // Show disabled marker
    if !elem.enabled() {
        line.push_str(" (disabled)");
    }

    println!("{}", line);
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
    fn test_is_last_sibling_basic() {
        let elements = vec![
            ElementInfo::new(
                "AXWindow".to_string(),
                None,
                None,
                None,
                0,
                0,
                0,
                0,
                true,
                0,
            ),
            ElementInfo::new(
                "AXButton".to_string(),
                None,
                None,
                None,
                0,
                0,
                0,
                0,
                true,
                1,
            ),
            ElementInfo::new(
                "AXButton".to_string(),
                None,
                None,
                None,
                0,
                0,
                0,
                0,
                true,
                1,
            ),
        ];
        // First child at depth 1 is not last
        assert!(!is_last_sibling(&elements, 1, 1));
        // Second child at depth 1 is last
        assert!(is_last_sibling(&elements, 2, 1));
    }

    #[test]
    fn test_is_last_sibling_nested() {
        let elements = vec![
            ElementInfo::new(
                "AXWindow".to_string(),
                None,
                None,
                None,
                0,
                0,
                0,
                0,
                true,
                0,
            ),
            ElementInfo::new("AXGroup".to_string(), None, None, None, 0, 0, 0, 0, true, 1),
            ElementInfo::new(
                "AXButton".to_string(),
                None,
                None,
                None,
                0,
                0,
                0,
                0,
                true,
                2,
            ),
            ElementInfo::new("AXGroup".to_string(), None, None, None, 0, 0, 0, 0, true, 1),
        ];
        // AXGroup at index 1 is NOT last at depth 1 (index 3 is at depth 1 too)
        assert!(!is_last_sibling(&elements, 1, 1));
        // AXButton at index 2 IS last at depth 2 (next element goes back to depth 1)
        assert!(is_last_sibling(&elements, 2, 2));
        // AXGroup at index 3 IS last at depth 1
        assert!(is_last_sibling(&elements, 3, 1));
    }

    #[test]
    fn test_build_tree_indent_depth_1() {
        let elements = vec![
            ElementInfo::new(
                "AXWindow".to_string(),
                None,
                None,
                None,
                0,
                0,
                0,
                0,
                true,
                0,
            ),
            ElementInfo::new(
                "AXButton".to_string(),
                None,
                None,
                None,
                0,
                0,
                0,
                0,
                true,
                1,
            ),
        ];
        // Depth 1 has no ancestor indents (level 1..1 is empty)
        assert_eq!(build_tree_indent(&elements, 1, 1), "");
    }

    #[test]
    fn test_build_tree_indent_depth_2_with_sibling() {
        let elements = vec![
            ElementInfo::new(
                "AXWindow".to_string(),
                None,
                None,
                None,
                0,
                0,
                0,
                0,
                true,
                0,
            ),
            ElementInfo::new("AXGroup".to_string(), None, None, None, 0, 0, 0, 0, true, 1),
            ElementInfo::new(
                "AXButton".to_string(),
                None,
                None,
                None,
                0,
                0,
                0,
                0,
                true,
                2,
            ),
            // Another element at depth 1 means the depth-1 ancestor has siblings
            ElementInfo::new("AXGroup".to_string(), None, None, None, 0, 0, 0, 0, true, 1),
        ];
        // At index 2 (depth 2), ancestor at level 1 still has siblings
        assert_eq!(build_tree_indent(&elements, 2, 2), "│   ");
    }

    #[test]
    fn test_build_tree_indent_depth_2_no_sibling() {
        let elements = vec![
            ElementInfo::new(
                "AXWindow".to_string(),
                None,
                None,
                None,
                0,
                0,
                0,
                0,
                true,
                0,
            ),
            ElementInfo::new("AXGroup".to_string(), None, None, None, 0, 0, 0, 0, true, 1),
            ElementInfo::new(
                "AXButton".to_string(),
                None,
                None,
                None,
                0,
                0,
                0,
                0,
                true,
                2,
            ),
        ];
        // At index 2 (depth 2), ancestor at level 1 has no more siblings
        assert_eq!(build_tree_indent(&elements, 2, 2), "    ");
    }

    #[test]
    fn test_tree_depth_gap() {
        // Depth gap: 0 → 2 (missing depth 1) — should not panic
        let elements = vec![
            ElementInfo::new(
                "AXWindow".to_string(),
                None,
                None,
                None,
                0,
                0,
                0,
                0,
                true,
                0,
            ),
            ElementInfo::new(
                "AXButton".to_string(),
                None,
                None,
                None,
                0,
                0,
                0,
                0,
                true,
                2,
            ),
        ];
        // Verify helpers don't panic on depth gaps
        assert!(is_last_sibling(&elements, 1, 2));
        assert_eq!(build_tree_indent(&elements, 1, 2), "    ");
        // Full render should not panic
        print_elements_tree(&elements);
    }

    #[test]
    fn test_tree_no_root_element() {
        // All elements at depth > 0
        let elements = vec![
            ElementInfo::new(
                "AXButton".to_string(),
                None,
                None,
                None,
                0,
                0,
                0,
                0,
                true,
                1,
            ),
            ElementInfo::new("AXGroup".to_string(), None, None, None, 0, 0, 0, 0, true, 1),
        ];
        // Should not panic — elements get connectors even without a depth-0 root
        assert!(!is_last_sibling(&elements, 0, 1));
        assert!(is_last_sibling(&elements, 1, 1));
        print_elements_tree(&elements);
    }

    #[test]
    fn test_tree_deep_nesting() {
        let elements = vec![
            ElementInfo::new("L0".to_string(), None, None, None, 0, 0, 0, 0, true, 0),
            ElementInfo::new("L1".to_string(), None, None, None, 0, 0, 0, 0, true, 1),
            ElementInfo::new("L2".to_string(), None, None, None, 0, 0, 0, 0, true, 2),
            ElementInfo::new("L3".to_string(), None, None, None, 0, 0, 0, 0, true, 3),
            ElementInfo::new("L4".to_string(), None, None, None, 0, 0, 0, 0, true, 4),
            ElementInfo::new("L5".to_string(), None, None, None, 0, 0, 0, 0, true, 5),
        ];
        let indent = build_tree_indent(&elements, 5, 5);
        // 4 ancestor levels (1..5), each "    " (no siblings), = 16 chars
        assert_eq!(indent.len(), 16);
        print_elements_tree(&elements);
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
