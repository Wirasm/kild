//! UI keybinding types: parsed, matchable shortcuts for kild-ui event handlers.
//!
//! Converts raw `kild_core::Keybindings` strings into types with a `matches()`
//! method for use in GPUI `on_key_down` handlers.

use gpui::{Keystroke, Modifiers};
use tracing::warn;

/// A parsed keyboard shortcut: a set of modifier flags plus a key name.
///
/// Created via `ParsedKeybinding::from_str("cmd+shift+[")`. Falls back to the
/// hardcoded default when the raw string is invalid.
///
/// ## Key-char gotcha
///
/// `gpui::Keystroke` derives `PartialEq` but includes a `key_char` field.
/// Real events have `key_char: Some(...)` while parsed keystrokes have
/// `key_char: None`, so direct `==` always fails. This type compares only the
/// fields that matter: `key`, `control`, `alt`, `shift`, `platform`.
#[derive(Clone)]
pub(crate) struct ParsedKeybinding {
    key: String,
    control: bool,
    alt: bool,
    shift: bool,
    platform: bool,
}

impl ParsedKeybinding {
    /// Parse a `"modifier+key"` string.
    ///
    /// Returns `None` and emits a `warn!` if a modifier token is unrecognised.
    /// Known modifiers: `ctrl`/`control`, `alt`/`option`, `shift`,
    /// `cmd`/`super`/`win`.
    pub(crate) fn from_str(s: &str) -> Option<Self> {
        if s.is_empty() {
            return None;
        }

        let parts: Vec<&str> = s.split('+').collect();
        let key = parts.last()?.to_lowercase();
        if key.is_empty() {
            return None;
        }

        let mut control = false;
        let mut alt = false;
        let mut shift = false;
        let mut platform = false;

        for modifier in &parts[..parts.len() - 1] {
            match modifier.to_lowercase().as_str() {
                "ctrl" | "control" => control = true,
                "alt" | "option" => alt = true,
                "shift" => shift = true,
                "cmd" | "super" | "win" => platform = true,
                other => {
                    warn!(
                        event = "ui.keybindings.unknown_modifier",
                        value = other,
                        binding = s,
                    );
                    return None;
                }
            }
        }

        Some(Self {
            key,
            control,
            alt,
            shift,
            platform,
        })
    }

    /// Returns `true` if this binding matches `keystroke`.
    ///
    /// Compares `key` (case-insensitive), `control`, `alt`, `shift`, and
    /// `platform` only — deliberately excludes `key_char` to avoid the
    /// real-event vs. parsed-event mismatch.
    pub(crate) fn matches(&self, keystroke: &Keystroke) -> bool {
        keystroke.key.to_lowercase() == self.key
            && keystroke.modifiers.control == self.control
            && keystroke.modifiers.alt == self.alt
            && keystroke.modifiers.shift == self.shift
            && keystroke.modifiers.platform == self.platform
    }

    /// Returns the binding in GPUI's `-`-separated hint format.
    ///
    /// For example: `ParsedKeybinding::from_str("cmd+shift+[")` returns
    /// `"cmd-shift-["`. Used by the status bar to render `Kbd` hints.
    pub(crate) fn hint_str(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if self.platform {
            parts.push("cmd");
        }
        if self.control {
            parts.push("ctrl");
        }
        if self.alt {
            parts.push("alt");
        }
        if self.shift {
            parts.push("shift");
        }
        parts.push(&self.key);
        parts.join("-")
    }
}

/// Parsed modifier for kild index jumping (modifier+1-9).
///
/// Replaces the old `NavModifier` type. Valid source strings: `"ctrl"`,
/// `"alt"`, `"cmd+shift"`.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ParsedJumpModifier {
    Ctrl,
    Alt,
    CmdShift,
}

impl ParsedJumpModifier {
    /// Parse a jump modifier string. Falls back to `Ctrl` and warns on unknown values.
    pub(crate) fn from_str(s: &str) -> Self {
        match s {
            "ctrl" => Self::Ctrl,
            "alt" => Self::Alt,
            "cmd+shift" => Self::CmdShift,
            other => {
                warn!(
                    event = "ui.keybindings.invalid_jump_modifier",
                    value = other,
                    valid_values = "ctrl, alt, cmd+shift",
                    "Invalid jump_modifier, using 'ctrl'"
                );
                Self::Ctrl
            }
        }
    }

    /// Return the display prefix for keyboard hints (e.g. `"ctrl"`, `"cmd-shift"`).
    pub(crate) fn hint_prefix(&self) -> &'static str {
        match self {
            Self::Ctrl => "ctrl",
            Self::Alt => "alt",
            Self::CmdShift => "cmd-shift",
        }
    }

    /// Returns `true` if `modifiers` matches this jump modifier combination.
    pub(crate) fn matches(&self, modifiers: &Modifiers) -> bool {
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

/// Parsed terminal keybindings.
#[derive(Clone)]
pub(crate) struct UiTerminalKeybindings {
    pub(crate) focus_escape: ParsedKeybinding,
    pub(crate) copy: ParsedKeybinding,
    pub(crate) paste: ParsedKeybinding,
}

/// Parsed navigation keybindings.
#[derive(Clone)]
pub(crate) struct UiNavigationKeybindings {
    pub(crate) next_kild: ParsedKeybinding,
    pub(crate) prev_kild: ParsedKeybinding,
    pub(crate) jump_modifier: ParsedJumpModifier,
    pub(crate) toggle_view: ParsedKeybinding,
    pub(crate) next_workspace: ParsedKeybinding,
    pub(crate) prev_workspace: ParsedKeybinding,
}

/// All parsed UI keybindings, ready for use in `on_key_down` handlers.
#[derive(Clone)]
pub(crate) struct UiKeybindings {
    pub(crate) terminal: UiTerminalKeybindings,
    pub(crate) navigation: UiNavigationKeybindings,
}

impl UiKeybindings {
    /// Build `UiKeybindings` from a `kild_core::Keybindings` config.
    ///
    /// Invalid binding strings warn and fall back to hardcoded defaults.
    pub(crate) fn from_config(keybindings: &kild_core::Keybindings) -> Self {
        let nav = &keybindings.navigation;
        let term = &keybindings.terminal;

        // Parse a binding string, falling back to `default` on invalid input.
        // `from_str` already warns about the unknown modifier; we just use the default.
        let parse_or_default = |s: &str, default: &str| -> ParsedKeybinding {
            ParsedKeybinding::from_str(s).unwrap_or_else(|| {
                ParsedKeybinding::from_str(default)
                    .expect("hardcoded default keybinding must parse")
            })
        };

        UiKeybindings {
            terminal: UiTerminalKeybindings {
                focus_escape: parse_or_default(term.focus_escape(), "ctrl+escape"),
                copy: parse_or_default(term.copy(), "cmd+c"),
                paste: parse_or_default(term.paste(), "cmd+v"),
            },
            navigation: UiNavigationKeybindings {
                next_kild: parse_or_default(nav.next_kild(), "cmd+j"),
                prev_kild: parse_or_default(nav.prev_kild(), "cmd+k"),
                jump_modifier: ParsedJumpModifier::from_str(nav.jump_modifier()),
                toggle_view: parse_or_default(nav.toggle_view(), "cmd+d"),
                next_workspace: parse_or_default(nav.next_workspace(), "cmd+shift+]"),
                prev_workspace: parse_or_default(nav.prev_workspace(), "cmd+shift+["),
            },
        }
    }

    /// Returns `UiKeybindings` with all default values.
    ///
    /// Used in tests.
    #[cfg(test)]
    pub(crate) fn default_bindings() -> Self {
        Self::from_config(&kild_core::Keybindings::default())
    }

    /// Returns `true` if `keystroke` matches any navigation shortcut that should
    /// be propagated from `TerminalView` to `MainView` rather than sent to the PTY.
    ///
    /// Includes `focus_escape` so `Ctrl+Escape` explicitly propagates instead of
    /// being written as `\x1b` to the terminal process.
    ///
    /// # Maintenance note
    ///
    /// This list must be kept in sync with all `ParsedKeybinding` fields in
    /// `UiNavigationKeybindings` plus terminal bindings that must not reach the PTY.
    /// If you add a new navigation binding, add it here too — the compiler won't
    /// remind you, and a missing entry silently passes keystrokes to the PTY.
    ///
    /// Note: jump-modifier shortcuts (modifier+1-9) use a separate digit-check path
    /// in rendering.rs and are not listed here.
    pub(crate) fn matches_any_nav_shortcut(&self, keystroke: &Keystroke) -> bool {
        self.navigation.next_kild.matches(keystroke)
            || self.navigation.prev_kild.matches(keystroke)
            || self.navigation.toggle_view.matches(keystroke)
            || self.navigation.next_workspace.matches(keystroke)
            || self.navigation.prev_workspace.matches(keystroke)
            || self.terminal.focus_escape.matches(keystroke)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Modifiers;

    fn make_keystroke(key: &str, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            key: key.into(),
            modifiers,
            ..Default::default()
        }
    }

    fn ctrl_mods() -> Modifiers {
        Modifiers {
            control: true,
            ..Default::default()
        }
    }

    fn cmd_mods() -> Modifiers {
        Modifiers {
            platform: true,
            ..Default::default()
        }
    }

    fn cmd_shift_mods() -> Modifiers {
        Modifiers {
            platform: true,
            shift: true,
            ..Default::default()
        }
    }

    // --- ParsedKeybinding::from_str ---

    #[test]
    fn test_from_str_simple_cmd_j() {
        let kb = ParsedKeybinding::from_str("cmd+j").unwrap();
        assert!(kb.platform);
        assert!(!kb.control);
        assert!(!kb.shift);
        assert_eq!(kb.key, "j");
    }

    #[test]
    fn test_from_str_ctrl_escape() {
        let kb = ParsedKeybinding::from_str("ctrl+escape").unwrap();
        assert!(kb.control);
        assert!(!kb.platform);
        assert_eq!(kb.key, "escape");
    }

    #[test]
    fn test_from_str_cmd_shift_bracket() {
        let kb = ParsedKeybinding::from_str("cmd+shift+[").unwrap();
        assert!(kb.platform);
        assert!(kb.shift);
        assert!(!kb.control);
        assert_eq!(kb.key, "[");
    }

    #[test]
    fn test_from_str_unknown_modifier_returns_none() {
        let result = ParsedKeybinding::from_str("typo+j");
        assert!(result.is_none());
    }

    #[test]
    fn test_from_str_empty_string_returns_none() {
        assert!(ParsedKeybinding::from_str("").is_none());
    }

    #[test]
    fn test_from_str_key_only_no_modifiers() {
        let kb = ParsedKeybinding::from_str("escape").unwrap();
        assert!(!kb.control);
        assert!(!kb.platform);
        assert!(!kb.shift);
        assert!(!kb.alt);
        assert_eq!(kb.key, "escape");
    }

    // --- ParsedKeybinding::matches ---

    #[test]
    fn test_matches_cmd_j() {
        let kb = ParsedKeybinding::from_str("cmd+j").unwrap();
        let ks = make_keystroke("j", cmd_mods());
        assert!(kb.matches(&ks));
    }

    #[test]
    fn test_matches_does_not_match_wrong_modifier() {
        let kb = ParsedKeybinding::from_str("cmd+j").unwrap();
        let ks = make_keystroke("j", ctrl_mods());
        assert!(!kb.matches(&ks));
    }

    #[test]
    fn test_matches_does_not_match_wrong_key() {
        let kb = ParsedKeybinding::from_str("cmd+j").unwrap();
        let ks = make_keystroke("k", cmd_mods());
        assert!(!kb.matches(&ks));
    }

    #[test]
    fn test_matches_ctrl_escape() {
        let kb = ParsedKeybinding::from_str("ctrl+escape").unwrap();
        let ks = make_keystroke("escape", ctrl_mods());
        assert!(kb.matches(&ks));
    }

    #[test]
    fn test_matches_cmd_shift_bracket() {
        let kb = ParsedKeybinding::from_str("cmd+shift+[").unwrap();
        let ks = make_keystroke("[", cmd_shift_mods());
        assert!(kb.matches(&ks));
    }

    // --- ParsedKeybinding::hint_str ---

    #[test]
    fn test_hint_str_cmd_j() {
        let kb = ParsedKeybinding::from_str("cmd+j").unwrap();
        assert_eq!(kb.hint_str(), "cmd-j");
    }

    #[test]
    fn test_hint_str_cmd_shift_bracket() {
        let kb = ParsedKeybinding::from_str("cmd+shift+[").unwrap();
        assert_eq!(kb.hint_str(), "cmd-shift-[");
    }

    #[test]
    fn test_hint_str_ctrl_escape() {
        let kb = ParsedKeybinding::from_str("ctrl+escape").unwrap();
        assert_eq!(kb.hint_str(), "ctrl-escape");
    }

    // --- ParsedJumpModifier ---

    #[test]
    fn test_jump_modifier_ctrl() {
        let m = ParsedJumpModifier::from_str("ctrl");
        assert!(m.matches(&ctrl_mods()));
        assert!(!m.matches(&cmd_mods()));
        assert_eq!(m.hint_prefix(), "ctrl");
    }

    #[test]
    fn test_jump_modifier_alt() {
        let m = ParsedJumpModifier::from_str("alt");
        let alt_mods = Modifiers {
            alt: true,
            ..Default::default()
        };
        assert!(m.matches(&alt_mods));
        assert!(!m.matches(&ctrl_mods()));
        assert_eq!(m.hint_prefix(), "alt");
    }

    #[test]
    fn test_jump_modifier_cmd_shift() {
        let m = ParsedJumpModifier::from_str("cmd+shift");
        assert!(m.matches(&cmd_shift_mods()));
        assert!(!m.matches(&ctrl_mods()));
        assert_eq!(m.hint_prefix(), "cmd-shift");
    }

    #[test]
    fn test_jump_modifier_invalid_falls_back_to_ctrl() {
        let m = ParsedJumpModifier::from_str("invalid");
        assert!(m.matches(&ctrl_mods()));
        assert_eq!(m.hint_prefix(), "ctrl");
    }

    // --- UiKeybindings ---

    #[test]
    fn test_default_bindings_are_valid() {
        let kb = UiKeybindings::default_bindings();
        // Spot-check defaults
        assert!(
            kb.navigation
                .next_kild
                .matches(&make_keystroke("j", cmd_mods()))
        );
        assert!(
            kb.navigation
                .prev_kild
                .matches(&make_keystroke("k", cmd_mods()))
        );
        assert!(
            kb.terminal
                .focus_escape
                .matches(&make_keystroke("escape", ctrl_mods()))
        );
    }

    #[test]
    fn test_matches_any_nav_shortcut_next_kild() {
        let kb = UiKeybindings::default_bindings();
        assert!(kb.matches_any_nav_shortcut(&make_keystroke("j", cmd_mods())));
    }

    #[test]
    fn test_matches_any_nav_shortcut_focus_escape() {
        let kb = UiKeybindings::default_bindings();
        assert!(kb.matches_any_nav_shortcut(&make_keystroke("escape", ctrl_mods())));
    }

    #[test]
    fn test_matches_any_nav_shortcut_no_match() {
        let kb = UiKeybindings::default_bindings();
        // Plain "a" with no modifiers is not a nav shortcut
        let ks = make_keystroke("a", Modifiers::default());
        assert!(!kb.matches_any_nav_shortcut(&ks));
    }

    #[test]
    fn test_from_config_custom_binding() {
        let mut raw = kild_core::Keybindings::default();
        raw.navigation.next_kild = Some("alt+j".to_string());

        let kb = UiKeybindings::from_config(&raw);
        let alt_mods = Modifiers {
            alt: true,
            ..Default::default()
        };
        assert!(
            kb.navigation
                .next_kild
                .matches(&make_keystroke("j", alt_mods))
        );
        // cmd+j should no longer match
        assert!(
            !kb.navigation
                .next_kild
                .matches(&make_keystroke("j", cmd_mods()))
        );
    }

    #[test]
    fn test_matches_any_nav_shortcut_respects_custom_binding() {
        // When next_kild is remapped to alt+j:
        // - alt+j must be intercepted (not sent to PTY)
        // - cmd+j must pass through (no longer a nav shortcut)
        let mut raw = kild_core::Keybindings::default();
        raw.navigation.next_kild = Some("alt+j".to_string());
        let kb = UiKeybindings::from_config(&raw);

        let alt_mods = Modifiers {
            alt: true,
            ..Default::default()
        };
        assert!(kb.matches_any_nav_shortcut(&make_keystroke("j", alt_mods)));
        assert!(!kb.matches_any_nav_shortcut(&make_keystroke("j", cmd_mods())));
    }

    #[test]
    fn test_from_config_invalid_binding_falls_back_to_default() {
        let mut raw = kild_core::Keybindings::default();
        raw.navigation.next_kild = Some("badmod+j".to_string());

        let kb = UiKeybindings::from_config(&raw);
        // Should fall back to "cmd+j"
        assert!(
            kb.navigation
                .next_kild
                .matches(&make_keystroke("j", cmd_mods()))
        );
    }
}
