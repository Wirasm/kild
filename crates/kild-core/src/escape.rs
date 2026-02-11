//! Shared string escaping utilities.

/// Escape a string for use in AppleScript.
pub fn applescript_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_applescript_escape() {
        assert_eq!(applescript_escape("hello"), "hello");
        assert_eq!(applescript_escape("hello\"world"), "hello\\\"world");
        assert_eq!(applescript_escape("hello\\world"), "hello\\\\world");
        assert_eq!(applescript_escape("hello\nworld"), "hello\\nworld");
    }
}
