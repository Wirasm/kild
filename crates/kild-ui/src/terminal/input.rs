use gpui::Keystroke;

/// Convert a GPUI Keystroke to terminal escape sequence bytes.
///
/// Returns `None` if the keystroke should not be sent to the terminal
/// (e.g., Ctrl+T is reserved for toggle, or the key is unrecognized).
///
/// `app_cursor_mode` changes arrow key encoding (needed for vim/tmux):
/// - Normal mode: `ESC [ A/B/C/D`
/// - App cursor mode: `ESC O A/B/C/D`
pub fn keystroke_to_escape(keystroke: &Keystroke, app_cursor_mode: bool) -> Option<Vec<u8>> {
    let key = keystroke.key.as_str();
    let ctrl = keystroke.modifiers.control;
    let alt = keystroke.modifiers.alt;

    // Ctrl+T / Ctrl+D are reserved for terminal toggle — propagate to parent
    if ctrl && (key == "t" || key == "d") {
        return None;
    }

    // Cmd+J/K/D: reserved for kild navigation (handled by MainView)
    let cmd = keystroke.modifiers.platform;
    if cmd && matches!(key, "j" | "k" | "d") {
        return None;
    }

    // Cmd+nav: macOS line-level shortcuts
    if cmd {
        return match key {
            "backspace" => Some(vec![0x15]), // Ctrl+U: delete to beginning of line
            "delete" => Some(vec![0x0b]),    // Ctrl+K: delete to end of line
            "left" => Some(vec![0x01]),      // Ctrl+A: beginning of line
            "right" => Some(vec![0x05]),     // Ctrl+E: end of line
            _ => None,                       // Other Cmd+key: propagate to parent
        };
    }

    // Alt+nav: macOS word-level shortcuts
    if alt {
        match key {
            "backspace" => return Some(vec![0x1b, 0x7f]), // ESC DEL: delete word backward
            "delete" => return Some(vec![0x1b, b'd']),    // ESC d: delete word forward
            "left" => return Some(vec![0x1b, b'b']),      // ESC b: word backward
            "right" => return Some(vec![0x1b, b'f']),     // ESC f: word forward
            _ => {} // Fall through to printable Alt+key handler
        }
    }

    // Ctrl+letter → ASCII control code (0x01-0x1A)
    if ctrl && key.len() == 1 {
        let ch = key.as_bytes()[0];
        if ch.is_ascii_lowercase() {
            let code = ch - b'a' + 1;
            return Some(vec![code]);
        }
    }

    // Named keys
    match key {
        "enter" => return Some(b"\r".to_vec()),
        "backspace" => return Some(vec![0x7f]),
        "tab" => {
            if keystroke.modifiers.shift {
                return Some(b"\x1b[Z".to_vec());
            }
            return Some(b"\t".to_vec());
        }
        "escape" => return Some(vec![0x1b]),
        "space" => return Some(b" ".to_vec()),
        "delete" => return Some(b"\x1b[3~".to_vec()),
        "home" => return Some(b"\x1b[H".to_vec()),
        "end" => return Some(b"\x1b[F".to_vec()),
        "pageup" => return Some(b"\x1b[5~".to_vec()),
        "pagedown" => return Some(b"\x1b[6~".to_vec()),
        "up" => {
            return Some(if app_cursor_mode {
                b"\x1bOA".to_vec()
            } else {
                b"\x1b[A".to_vec()
            });
        }
        "down" => {
            return Some(if app_cursor_mode {
                b"\x1bOB".to_vec()
            } else {
                b"\x1b[B".to_vec()
            });
        }
        "right" => {
            return Some(if app_cursor_mode {
                b"\x1bOC".to_vec()
            } else {
                b"\x1b[C".to_vec()
            });
        }
        "left" => {
            return Some(if app_cursor_mode {
                b"\x1bOD".to_vec()
            } else {
                b"\x1b[D".to_vec()
            });
        }
        _ => {}
    }

    // Printable characters (skip function keys like f1–f12)
    let is_function_key =
        key.starts_with("f") && key.len() > 1 && key[1..].bytes().all(|b| b.is_ascii_digit());
    if !ctrl && !key.is_empty() && !is_function_key {
        let mut bytes = key.as_bytes().to_vec();
        // Alt+key wraps with ESC prefix
        if alt {
            bytes.insert(0, 0x1b);
        }
        return Some(bytes);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Modifiers;

    fn key(name: &str) -> Keystroke {
        Keystroke {
            key: name.into(),
            modifiers: Modifiers::default(),
            ..Default::default()
        }
    }

    fn ctrl_key(name: &str) -> Keystroke {
        Keystroke {
            key: name.into(),
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn alt_key(name: &str) -> Keystroke {
        Keystroke {
            key: name.into(),
            modifiers: Modifiers {
                alt: true,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn shift_key(name: &str) -> Keystroke {
        Keystroke {
            key: name.into(),
            modifiers: Modifiers {
                shift: true,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn test_printable_characters() {
        assert_eq!(keystroke_to_escape(&key("a"), false), Some(b"a".to_vec()));
        assert_eq!(keystroke_to_escape(&key("f"), false), Some(b"f".to_vec()));
        assert_eq!(keystroke_to_escape(&key("z"), false), Some(b"z".to_vec()));
        assert_eq!(keystroke_to_escape(&key("1"), false), Some(b"1".to_vec()));
    }

    #[test]
    fn test_function_keys_return_none() {
        assert_eq!(keystroke_to_escape(&key("f1"), false), None);
        assert_eq!(keystroke_to_escape(&key("f12"), false), None);
    }

    #[test]
    fn test_enter_returns_cr() {
        assert_eq!(
            keystroke_to_escape(&key("enter"), false),
            Some(b"\r".to_vec())
        );
    }

    #[test]
    fn test_backspace_returns_del() {
        assert_eq!(
            keystroke_to_escape(&key("backspace"), false),
            Some(vec![0x7f])
        );
    }

    #[test]
    fn test_tab() {
        assert_eq!(
            keystroke_to_escape(&key("tab"), false),
            Some(b"\t".to_vec())
        );
    }

    #[test]
    fn test_shift_tab() {
        assert_eq!(
            keystroke_to_escape(&shift_key("tab"), false),
            Some(b"\x1b[Z".to_vec())
        );
    }

    #[test]
    fn test_escape_key() {
        assert_eq!(keystroke_to_escape(&key("escape"), false), Some(vec![0x1b]));
    }

    #[test]
    fn test_arrow_keys_normal_mode() {
        assert_eq!(
            keystroke_to_escape(&key("up"), false),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            keystroke_to_escape(&key("down"), false),
            Some(b"\x1b[B".to_vec())
        );
        assert_eq!(
            keystroke_to_escape(&key("right"), false),
            Some(b"\x1b[C".to_vec())
        );
        assert_eq!(
            keystroke_to_escape(&key("left"), false),
            Some(b"\x1b[D".to_vec())
        );
    }

    #[test]
    fn test_arrow_keys_app_cursor_mode() {
        assert_eq!(
            keystroke_to_escape(&key("up"), true),
            Some(b"\x1bOA".to_vec())
        );
        assert_eq!(
            keystroke_to_escape(&key("down"), true),
            Some(b"\x1bOB".to_vec())
        );
        assert_eq!(
            keystroke_to_escape(&key("right"), true),
            Some(b"\x1bOC".to_vec())
        );
        assert_eq!(
            keystroke_to_escape(&key("left"), true),
            Some(b"\x1bOD".to_vec())
        );
    }

    #[test]
    fn test_ctrl_c() {
        assert_eq!(keystroke_to_escape(&ctrl_key("c"), false), Some(vec![0x03]));
    }

    #[test]
    fn test_ctrl_d_returns_none() {
        // Ctrl+D is reserved for daemon terminal toggle
        assert_eq!(keystroke_to_escape(&ctrl_key("d"), false), None);
    }

    #[test]
    fn test_ctrl_z() {
        assert_eq!(keystroke_to_escape(&ctrl_key("z"), false), Some(vec![0x1a]));
    }

    #[test]
    fn test_ctrl_t_returns_none() {
        assert_eq!(keystroke_to_escape(&ctrl_key("t"), false), None);
    }

    #[test]
    fn test_home_end() {
        assert_eq!(
            keystroke_to_escape(&key("home"), false),
            Some(b"\x1b[H".to_vec())
        );
        assert_eq!(
            keystroke_to_escape(&key("end"), false),
            Some(b"\x1b[F".to_vec())
        );
    }

    #[test]
    fn test_page_up_down() {
        assert_eq!(
            keystroke_to_escape(&key("pageup"), false),
            Some(b"\x1b[5~".to_vec())
        );
        assert_eq!(
            keystroke_to_escape(&key("pagedown"), false),
            Some(b"\x1b[6~".to_vec())
        );
    }

    #[test]
    fn test_delete() {
        assert_eq!(
            keystroke_to_escape(&key("delete"), false),
            Some(b"\x1b[3~".to_vec())
        );
    }

    #[test]
    fn test_alt_key_wraps_with_esc() {
        let result = keystroke_to_escape(&alt_key("b"), false);
        assert_eq!(result, Some(vec![0x1b, b'b']));
    }

    #[test]
    fn test_space() {
        assert_eq!(
            keystroke_to_escape(&key("space"), false),
            Some(b" ".to_vec())
        );
    }

    fn cmd_key(name: &str) -> Keystroke {
        Keystroke {
            key: name.into(),
            modifiers: Modifiers {
                platform: true,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn test_cmd_j_returns_none() {
        assert_eq!(keystroke_to_escape(&cmd_key("j"), false), None);
    }

    #[test]
    fn test_cmd_k_returns_none() {
        assert_eq!(keystroke_to_escape(&cmd_key("k"), false), None);
    }

    #[test]
    fn test_cmd_d_returns_none() {
        assert_eq!(keystroke_to_escape(&cmd_key("d"), false), None);
    }

    #[test]
    fn test_cmd_backspace_deletes_to_line_start() {
        assert_eq!(
            keystroke_to_escape(&cmd_key("backspace"), false),
            Some(vec![0x15])
        );
    }

    #[test]
    fn test_cmd_delete_deletes_to_line_end() {
        assert_eq!(
            keystroke_to_escape(&cmd_key("delete"), false),
            Some(vec![0x0b])
        );
    }

    #[test]
    fn test_cmd_left_jumps_to_line_start() {
        assert_eq!(
            keystroke_to_escape(&cmd_key("left"), false),
            Some(vec![0x01])
        );
    }

    #[test]
    fn test_cmd_right_jumps_to_line_end() {
        assert_eq!(
            keystroke_to_escape(&cmd_key("right"), false),
            Some(vec![0x05])
        );
    }

    #[test]
    fn test_alt_backspace_deletes_word() {
        assert_eq!(
            keystroke_to_escape(&alt_key("backspace"), false),
            Some(vec![0x1b, 0x7f])
        );
    }

    #[test]
    fn test_alt_left_moves_word_backward() {
        assert_eq!(
            keystroke_to_escape(&alt_key("left"), false),
            Some(vec![0x1b, b'b'])
        );
    }

    #[test]
    fn test_alt_delete_deletes_word_forward() {
        assert_eq!(
            keystroke_to_escape(&alt_key("delete"), false),
            Some(vec![0x1b, b'd'])
        );
    }

    #[test]
    fn test_alt_right_moves_word_forward() {
        assert_eq!(
            keystroke_to_escape(&alt_key("right"), false),
            Some(vec![0x1b, b'f'])
        );
    }

    #[test]
    fn test_cmd_other_keys_propagate() {
        assert_eq!(keystroke_to_escape(&cmd_key("z"), false), None);
        assert_eq!(keystroke_to_escape(&cmd_key("a"), false), None);
    }
}
