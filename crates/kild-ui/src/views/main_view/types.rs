//! Type definitions for the main view.

use tracing::warn;

/// Tracks which region of the UI currently has logical focus.
///
/// Used for keyboard routing — determines where key events are dispatched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusRegion {
    Dashboard,
    Terminal,
}

/// Which view is showing in the main area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActiveView {
    /// Terminal tabs per kild (default).
    Control,
    /// Fleet overview with kild cards.
    Dashboard,
    /// Kild detail drill-down (from dashboard card click).
    Detail,
}

/// Parsed modifier for kild index jumping (from config `[ui] nav_modifier`).
#[derive(Debug, Clone, Copy)]
pub(crate) enum NavModifier {
    Ctrl,
    Alt,
    CmdShift,
}

impl NavModifier {
    pub(crate) fn from_config(s: &str) -> Self {
        match s {
            "ctrl" => Self::Ctrl,
            "alt" => Self::Alt,
            "cmd+shift" => Self::CmdShift,
            other => {
                warn!(
                    event = "ui.config.invalid_nav_modifier",
                    value = other,
                    valid_values = "ctrl, alt, cmd+shift",
                    "Invalid nav_modifier in config, using 'ctrl'"
                );
                Self::Ctrl
            }
        }
    }

    /// Return the display string for keyboard hints.
    pub(crate) fn hint_prefix(&self) -> &'static str {
        match self {
            Self::Ctrl => "ctrl",
            Self::Alt => "alt",
            Self::CmdShift => "cmd-shift",
        }
    }

    pub(crate) fn matches(&self, modifiers: &gpui::Modifiers) -> bool {
        match self {
            Self::Ctrl => {
                modifiers.control && !modifiers.shift && !modifiers.alt && !modifiers.platform
            }
            Self::Alt => {
                modifiers.alt && !modifiers.control && !modifiers.shift && !modifiers.platform
            }
            Self::CmdShift => {
                modifiers.platform && modifiers.shift && !modifiers.control && !modifiers.alt
            }
        }
    }
}
