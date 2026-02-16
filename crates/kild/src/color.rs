//! Centralized CLI color functions using the Tallinn Night brand palette.
//!
//! All functions automatically respect `NO_COLOR`, `FORCE_COLOR`, and TTY detection
//! via `owo-colors`' `if_supports_color()`.

use std::sync::atomic::{AtomicBool, Ordering};

use owo_colors::OwoColorize;
use owo_colors::Stream::Stdout;

/// Global override: when true, forces color off (set by `--no-color` flag).
static NO_COLOR_FLAG: AtomicBool = AtomicBool::new(false);

/// Call once from main.rs when `--no-color` is passed.
pub fn set_no_color() {
    // Set the env var so supports-color picks it up for all subsequent calls.
    // SAFETY: Called once at startup before any threads are spawned.
    unsafe { std::env::set_var("NO_COLOR", "1") };
    NO_COLOR_FLAG.store(true, Ordering::Relaxed);
}

// =============================================================================
// TALLINN NIGHT PALETTE (RGB values from kild-ui/src/theme.rs)
// =============================================================================

// Ice (primary accent): #7CB4C8
const ICE: (u8, u8, u8) = (124, 180, 200);

// Aurora (active/success): #6B8F5E
const AURORA: (u8, u8, u8) = (107, 143, 94);

// Copper (warning/idle): #C49A5C
const COPPER: (u8, u8, u8) = (196, 154, 92);

// Ember (error/danger): #B87060
const EMBER: (u8, u8, u8) = (184, 112, 96);

// Kiri (AI/agent): #A088B0
const KIRI: (u8, u8, u8) = (160, 136, 176);

// Text muted: #5C6370
const MUTED: (u8, u8, u8) = (92, 99, 112);

// =============================================================================
// COLOR FUNCTIONS
// =============================================================================

/// Apply ice blue (branch names, primary accent).
pub fn ice(text: &str) -> String {
    text.if_supports_color(Stdout, |t| t.truecolor(ICE.0, ICE.1, ICE.2))
        .to_string()
}

/// Apply aurora green (active/success).
pub fn aurora(text: &str) -> String {
    text.if_supports_color(Stdout, |t| t.truecolor(AURORA.0, AURORA.1, AURORA.2))
        .to_string()
}

/// Apply copper amber (warning/idle).
pub fn copper(text: &str) -> String {
    text.if_supports_color(Stdout, |t| t.truecolor(COPPER.0, COPPER.1, COPPER.2))
        .to_string()
}

/// Apply ember red (error/danger).
pub fn ember(text: &str) -> String {
    text.if_supports_color(Stdout, |t| t.truecolor(EMBER.0, EMBER.1, EMBER.2))
        .to_string()
}

/// Apply kiri purple (agent/AI).
pub fn kiri(text: &str) -> String {
    text.if_supports_color(Stdout, |t| t.truecolor(KIRI.0, KIRI.1, KIRI.2))
        .to_string()
}

/// Apply bold bright text (headers).
pub fn bold(text: &str) -> String {
    text.if_supports_color(Stdout, |t| t.bold()).to_string()
}

/// Apply muted gray (secondary info, borders, hints).
pub fn muted(text: &str) -> String {
    text.if_supports_color(Stdout, |t| t.truecolor(MUTED.0, MUTED.1, MUTED.2))
        .to_string()
}

/// Color-code a session status value (active/stopped).
pub fn status(status_str: &str) -> String {
    match status_str {
        "active" => aurora(status_str),
        "stopped" => muted(status_str),
        "destroyed" => ember(status_str),
        _ => status_str.to_string(),
    }
}

/// Color-code an agent activity value (working/idle/waiting/error/done).
pub fn activity(activity_str: &str) -> String {
    match activity_str {
        "working" => kiri(activity_str),
        "idle" => copper(activity_str),
        "waiting" => copper(activity_str),
        "error" => ember(activity_str),
        "done" => aurora(activity_str),
        "-" => muted(activity_str),
        _ => activity_str.to_string(),
    }
}

/// Apply error styling (ember red, for stderr messages).
pub fn error(text: &str) -> String {
    // Use Stderr stream detection for error output
    text.if_supports_color(owo_colors::Stream::Stderr, |t| {
        t.truecolor(EMBER.0, EMBER.1, EMBER.2)
    })
    .to_string()
}

/// Apply warning styling (copper amber, for stderr messages).
pub fn warning(text: &str) -> String {
    text.if_supports_color(owo_colors::Stream::Stderr, |t| {
        t.truecolor(COPPER.0, COPPER.1, COPPER.2)
    })
    .to_string()
}

/// Apply hint styling (muted gray, for secondary info on stderr).
pub fn hint(text: &str) -> String {
    text.if_supports_color(owo_colors::Stream::Stderr, |t| {
        t.truecolor(MUTED.0, MUTED.1, MUTED.2)
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_functions_return_non_empty() {
        // All color functions should return non-empty strings
        assert!(!ice("test").is_empty());
        assert!(!aurora("test").is_empty());
        assert!(!copper("test").is_empty());
        assert!(!ember("test").is_empty());
        assert!(!kiri("test").is_empty());
        assert!(!bold("test").is_empty());
        assert!(!muted("test").is_empty());
        assert!(!error("test").is_empty());
        assert!(!warning("test").is_empty());
        assert!(!hint("test").is_empty());
    }

    #[test]
    fn test_status_maps_correctly() {
        // When NO_COLOR is set (in CI), these just return the plain text
        let active = status("active");
        assert!(active.contains("active"));

        let stopped = status("stopped");
        assert!(stopped.contains("stopped"));

        let unknown = status("something");
        assert_eq!(unknown, "something");
    }

    #[test]
    fn test_activity_maps_correctly() {
        let working = activity("working");
        assert!(working.contains("working"));

        let idle = activity("idle");
        assert!(idle.contains("idle"));

        let error_act = activity("error");
        assert!(error_act.contains("error"));

        let dash = activity("-");
        assert!(dash.contains("-"));

        let unknown = activity("other");
        assert_eq!(unknown, "other");
    }

    #[test]
    fn test_color_functions_contain_original_text() {
        // Regardless of color support, the original text must be present
        assert!(ice("branch-name").contains("branch-name"));
        assert!(aurora("active").contains("active"));
        assert!(ember("error msg").contains("error msg"));
        assert!(bold("Header").contains("Header"));
    }
}
