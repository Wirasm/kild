//! Centralized CLI color functions using the Tallinn Night brand palette.
//!
//! All functions automatically respect `NO_COLOR`, `FORCE_COLOR`, and TTY detection
//! via `owo-colors`' `if_supports_color()`. The `--no-color` flag sets an internal
//! flag that bypasses owo-colors entirely (no unsafe env mutation needed).

use std::sync::atomic::{AtomicBool, Ordering};

use owo_colors::OwoColorize;
use owo_colors::Stream::Stdout;

/// Global override: when true, forces color off (set by `--no-color` flag).
static NO_COLOR_FLAG: AtomicBool = AtomicBool::new(false);

/// Call once from main.rs when `--no-color` is passed.
///
/// Sets an in-process flag checked by all color functions. No environment
/// mutation — the pre-existing `NO_COLOR` env var is handled separately
/// by owo-colors at the library level.
pub fn set_no_color() {
    NO_COLOR_FLAG.store(true, Ordering::Relaxed);
}

// =============================================================================
// TALLINN NIGHT PALETTE — hex codes are the single source of truth.
// =============================================================================

/// Type-safe RGB color with compile-time hex-to-component conversion.
#[derive(Debug, Clone, Copy)]
struct Rgb {
    r: u8,
    g: u8,
    b: u8,
}

impl Rgb {
    const fn from_hex(hex: u32) -> Self {
        Self {
            r: ((hex >> 16) & 0xFF) as u8,
            g: ((hex >> 8) & 0xFF) as u8,
            b: (hex & 0xFF) as u8,
        }
    }
}

const ICE: Rgb = Rgb::from_hex(0x7CB4C8); // Primary accent
const AURORA: Rgb = Rgb::from_hex(0x6B8F5E); // Active/success
const COPPER: Rgb = Rgb::from_hex(0xC49A5C); // Warning/idle
const EMBER: Rgb = Rgb::from_hex(0xB87060); // Error/danger
const KIRI: Rgb = Rgb::from_hex(0xA088B0); // AI/agent
const MUTED: Rgb = Rgb::from_hex(0x5C6370); // Secondary info

// =============================================================================
// COLOR FUNCTIONS
// =============================================================================

/// Returns true when color output is disabled (--no-color flag).
fn no_color() -> bool {
    NO_COLOR_FLAG.load(Ordering::Relaxed)
}

/// Apply ice blue (branch names, primary accent).
pub fn ice(text: &str) -> String {
    if no_color() {
        return text.to_string();
    }
    text.if_supports_color(Stdout, |t| t.truecolor(ICE.r, ICE.g, ICE.b))
        .to_string()
}

/// Apply aurora green (active/success).
pub fn aurora(text: &str) -> String {
    if no_color() {
        return text.to_string();
    }
    text.if_supports_color(Stdout, |t| t.truecolor(AURORA.r, AURORA.g, AURORA.b))
        .to_string()
}

/// Apply copper amber (warning/idle).
pub fn copper(text: &str) -> String {
    if no_color() {
        return text.to_string();
    }
    text.if_supports_color(Stdout, |t| t.truecolor(COPPER.r, COPPER.g, COPPER.b))
        .to_string()
}

/// Apply ember red (error/danger).
pub fn ember(text: &str) -> String {
    if no_color() {
        return text.to_string();
    }
    text.if_supports_color(Stdout, |t| t.truecolor(EMBER.r, EMBER.g, EMBER.b))
        .to_string()
}

/// Apply kiri purple (agent/AI).
pub fn kiri(text: &str) -> String {
    if no_color() {
        return text.to_string();
    }
    text.if_supports_color(Stdout, |t| t.truecolor(KIRI.r, KIRI.g, KIRI.b))
        .to_string()
}

/// Apply bold bright text (headers).
pub fn bold(text: &str) -> String {
    if no_color() {
        return text.to_string();
    }
    text.if_supports_color(Stdout, |t| t.bold()).to_string()
}

/// Apply muted gray (secondary info, borders, hints).
pub fn muted(text: &str) -> String {
    if no_color() {
        return text.to_string();
    }
    text.if_supports_color(Stdout, |t| t.truecolor(MUTED.r, MUTED.g, MUTED.b))
        .to_string()
}

/// Color-code a session status value (active/stopped/destroyed).
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
    if no_color() {
        return text.to_string();
    }
    text.if_supports_color(owo_colors::Stream::Stderr, |t| {
        t.truecolor(EMBER.r, EMBER.g, EMBER.b)
    })
    .to_string()
}

/// Apply warning styling (copper amber, for stderr messages).
pub fn warning(text: &str) -> String {
    if no_color() {
        return text.to_string();
    }
    text.if_supports_color(owo_colors::Stream::Stderr, |t| {
        t.truecolor(COPPER.r, COPPER.g, COPPER.b)
    })
    .to_string()
}

/// Apply hint styling (muted gray, for secondary info on stderr).
pub fn hint(text: &str) -> String {
    if no_color() {
        return text.to_string();
    }
    text.if_supports_color(owo_colors::Stream::Stderr, |t| {
        t.truecolor(MUTED.r, MUTED.g, MUTED.b)
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb_from_hex() {
        let c = Rgb::from_hex(0x7CB4C8);
        assert_eq!(c.r, 124);
        assert_eq!(c.g, 180);
        assert_eq!(c.b, 200);
    }

    #[test]
    fn test_rgb_from_hex_black() {
        let c = Rgb::from_hex(0x000000);
        assert_eq!(c.r, 0);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);
    }

    #[test]
    fn test_rgb_from_hex_white() {
        let c = Rgb::from_hex(0xFFFFFF);
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 255);
        assert_eq!(c.b, 255);
    }

    #[test]
    fn test_palette_constants_match_hex() {
        // Ice: #7CB4C8
        assert_eq!(ICE.r, 0x7C);
        assert_eq!(ICE.g, 0xB4);
        assert_eq!(ICE.b, 0xC8);

        // Aurora: #6B8F5E
        assert_eq!(AURORA.r, 0x6B);
        assert_eq!(AURORA.g, 0x8F);
        assert_eq!(AURORA.b, 0x5E);

        // Copper: #C49A5C
        assert_eq!(COPPER.r, 0xC4);
        assert_eq!(COPPER.g, 0x9A);
        assert_eq!(COPPER.b, 0x5C);

        // Ember: #B87060
        assert_eq!(EMBER.r, 0xB8);
        assert_eq!(EMBER.g, 0x70);
        assert_eq!(EMBER.b, 0x60);

        // Kiri: #A088B0
        assert_eq!(KIRI.r, 0xA0);
        assert_eq!(KIRI.g, 0x88);
        assert_eq!(KIRI.b, 0xB0);

        // Muted: #5C6370
        assert_eq!(MUTED.r, 0x5C);
        assert_eq!(MUTED.g, 0x63);
        assert_eq!(MUTED.b, 0x70);
    }

    #[test]
    fn test_no_color_flag_disables_all_formatting() {
        // Set the flag, verify plain text returned
        NO_COLOR_FLAG.store(true, Ordering::Relaxed);

        assert_eq!(ice("test"), "test");
        assert_eq!(aurora("test"), "test");
        assert_eq!(copper("test"), "test");
        assert_eq!(ember("test"), "test");
        assert_eq!(kiri("test"), "test");
        assert_eq!(bold("test"), "test");
        assert_eq!(muted("test"), "test");
        assert_eq!(error("test"), "test");
        assert_eq!(warning("test"), "test");
        assert_eq!(hint("test"), "test");

        // Reset for other tests
        NO_COLOR_FLAG.store(false, Ordering::Relaxed);
    }

    #[test]
    fn test_color_functions_return_non_empty() {
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
        let active = status("active");
        assert!(active.contains("active"));

        let stopped = status("stopped");
        assert!(stopped.contains("stopped"));

        let destroyed = status("destroyed");
        assert!(destroyed.contains("destroyed"));

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

        let done = activity("done");
        assert!(done.contains("done"));

        let dash = activity("-");
        assert!(dash.contains("-"));

        let unknown = activity("other");
        assert_eq!(unknown, "other");
    }

    #[test]
    fn test_color_functions_contain_original_text() {
        assert!(ice("branch-name").contains("branch-name"));
        assert!(aurora("active").contains("active"));
        assert!(ember("error msg").contains("error msg"));
        assert!(bold("Header").contains("Header"));
    }
}
