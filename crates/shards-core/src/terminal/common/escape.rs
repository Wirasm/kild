//! String escaping utilities for terminal commands.

use std::path::Path;

/// Escape a string for use in shell commands (single-quoted).
pub fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

/// Escape a string for use in AppleScript.
pub fn applescript_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// Escape special regex characters for use in pkill -f pattern.
pub fn escape_regex(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        match c {
            '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '^' | '$' | '\\' => {
                result.push('\\');
                result.push(c);
            }
            _ => result.push(c),
        }
    }
    result
}

/// Build a shell command that changes to the working directory and executes the command.
pub fn build_cd_command(working_directory: &Path, command: &str) -> String {
    format!(
        "cd {} && {}",
        shell_escape(&working_directory.display().to_string()),
        command
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_shell_escape() {
        assert_eq!(shell_escape("hello"), "'hello'");
        assert_eq!(shell_escape("hello world"), "'hello world'");
        assert_eq!(shell_escape("hello'world"), "'hello'\"'\"'world'");
    }

    #[test]
    fn test_shell_escape_handles_metacharacters() {
        assert_eq!(shell_escape("path with spaces"), "'path with spaces'");
        assert_eq!(shell_escape("$HOME/dir"), "'$HOME/dir'");
        assert_eq!(shell_escape("dir;rm -rf /"), "'dir;rm -rf /'");
        assert_eq!(shell_escape("$(whoami)"), "'$(whoami)'");
        assert_eq!(shell_escape("`id`"), "'`id`'");
    }

    #[test]
    fn test_applescript_escape() {
        assert_eq!(applescript_escape("hello"), "hello");
        assert_eq!(applescript_escape("hello\"world"), "hello\\\"world");
        assert_eq!(applescript_escape("hello\\world"), "hello\\\\world");
        assert_eq!(applescript_escape("hello\nworld"), "hello\\nworld");
    }

    #[test]
    fn test_escape_regex_simple() {
        assert_eq!(escape_regex("hello"), "hello");
        assert_eq!(escape_regex("hello-world"), "hello-world");
        assert_eq!(escape_regex("hello_world_123"), "hello_world_123");
    }

    #[test]
    fn test_escape_regex_metacharacters() {
        assert_eq!(escape_regex("."), "\\.");
        assert_eq!(escape_regex("*"), "\\*");
        assert_eq!(escape_regex("+"), "\\+");
        assert_eq!(escape_regex("?"), "\\?");
        assert_eq!(escape_regex("("), "\\(");
        assert_eq!(escape_regex(")"), "\\)");
        assert_eq!(escape_regex("["), "\\[");
        assert_eq!(escape_regex("]"), "\\]");
        assert_eq!(escape_regex("{"), "\\{");
        assert_eq!(escape_regex("}"), "\\}");
        assert_eq!(escape_regex("|"), "\\|");
        assert_eq!(escape_regex("^"), "\\^");
        assert_eq!(escape_regex("$"), "\\$");
        assert_eq!(escape_regex("\\"), "\\\\");
    }

    #[test]
    fn test_escape_regex_mixed() {
        assert_eq!(escape_regex("shards-session"), "shards-session");
        assert_eq!(escape_regex("session.1"), "session\\.1");
        assert_eq!(escape_regex("test[0]"), "test\\[0\\]");
        assert_eq!(escape_regex("foo*bar"), "foo\\*bar");
    }

    #[test]
    fn test_build_cd_command() {
        let path = PathBuf::from("/tmp/test");
        let command = "echo hello";
        let result = build_cd_command(&path, command);
        assert!(result.contains("cd '/tmp/test'"));
        assert!(result.contains("&& echo hello"));
    }

    #[test]
    fn test_build_cd_command_with_spaces() {
        let path = PathBuf::from("/tmp/test with spaces");
        let command = "claude code";
        let result = build_cd_command(&path, command);
        assert!(result.contains("cd '/tmp/test with spaces'"));
        assert!(result.contains("&& claude code"));
    }
}
