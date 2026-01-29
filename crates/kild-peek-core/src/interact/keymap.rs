use core_graphics::event::CGEventFlags;

use super::errors::InteractionError;

/// A resolved key mapping with virtual keycode and modifier flags
#[derive(Debug, Clone)]
pub struct KeyMapping {
    pub keycode: u16,
    pub flags: CGEventFlags,
}

/// Parse a key combo string like "cmd+s", "enter", "cmd+shift+p" into a KeyMapping.
///
/// Supports:
/// - Single keys: "enter", "tab", "escape", "space", "delete", "up", "down", etc.
/// - Modifiers: "cmd"/"command", "shift", "ctrl"/"control", "opt"/"option"/"alt"
/// - Combos: "cmd+s", "cmd+shift+p", "ctrl+c", "alt+tab"
/// - Letter keys: "a"-"z"
/// - Number keys: "0"-"9"
/// - Function keys: "f1"-"f12"
///
/// Parsing is case-insensitive.
pub fn parse_key_combo(combo: &str) -> Result<KeyMapping, InteractionError> {
    let parts: Vec<&str> = combo.split('+').map(|s| s.trim()).collect();

    let mut flags = CGEventFlags::CGEventFlagNull;
    let mut key_name: Option<&str> = None;

    for part in &parts {
        let lower = part.to_lowercase();
        match lower.as_str() {
            "cmd" | "command" => flags |= CGEventFlags::CGEventFlagCommand,
            "shift" => flags |= CGEventFlags::CGEventFlagShift,
            "ctrl" | "control" => flags |= CGEventFlags::CGEventFlagControl,
            "opt" | "option" | "alt" => flags |= CGEventFlags::CGEventFlagAlternate,
            _ => {
                if key_name.is_some() {
                    return Err(InteractionError::UnknownKey {
                        name: combo.to_string(),
                    });
                }
                key_name = Some(part);
            }
        }
    }

    let key = key_name.ok_or_else(|| InteractionError::UnknownKey {
        name: combo.to_string(),
    })?;

    let keycode = resolve_keycode(key)?;

    Ok(KeyMapping { keycode, flags })
}

/// Resolve a key name to its macOS virtual keycode
fn resolve_keycode(name: &str) -> Result<u16, InteractionError> {
    let lower = name.to_lowercase();
    match lower.as_str() {
        // Letters (a=0, s=1, d=2, f=3, h=4, g=5, z=6, x=7, c=8, v=9,
        //          b=11, q=12, w=13, e=14, r=15, y=16, t=17, u=32, i=34,
        //          p=35, l=37, j=38, k=40, n=45, m=46, o=31)
        "a" => Ok(0),
        "s" => Ok(1),
        "d" => Ok(2),
        "f" => Ok(3),
        "h" => Ok(4),
        "g" => Ok(5),
        "z" => Ok(6),
        "x" => Ok(7),
        "c" => Ok(8),
        "v" => Ok(9),
        "b" => Ok(11),
        "q" => Ok(12),
        "w" => Ok(13),
        "e" => Ok(14),
        "r" => Ok(15),
        "y" => Ok(16),
        "t" => Ok(17),
        "1" => Ok(18),
        "2" => Ok(19),
        "3" => Ok(20),
        "4" => Ok(21),
        "5" => Ok(23),
        "6" => Ok(22),
        "7" => Ok(26),
        "8" => Ok(28),
        "9" => Ok(25),
        "0" => Ok(29),
        "o" => Ok(31),
        "u" => Ok(32),
        "i" => Ok(34),
        "p" => Ok(35),
        "l" => Ok(37),
        "j" => Ok(38),
        "k" => Ok(40),
        "n" => Ok(45),
        "m" => Ok(46),

        // Special keys
        "return" | "enter" => Ok(36),
        "tab" => Ok(48),
        "space" => Ok(49),
        "delete" | "backspace" => Ok(51),
        "escape" | "esc" => Ok(53),
        "forwarddelete" => Ok(117),

        // Arrow keys
        "left" => Ok(123),
        "right" => Ok(124),
        "down" => Ok(125),
        "up" => Ok(126),

        // Function keys
        "f1" => Ok(122),
        "f2" => Ok(120),
        "f3" => Ok(99),
        "f4" => Ok(118),
        "f5" => Ok(96),
        "f6" => Ok(97),
        "f7" => Ok(98),
        "f8" => Ok(100),
        "f9" => Ok(101),
        "f10" => Ok(109),
        "f11" => Ok(103),
        "f12" => Ok(111),

        // Punctuation
        "-" | "minus" => Ok(27),
        "=" | "equal" | "equals" => Ok(24),
        "[" | "leftbracket" => Ok(33),
        "]" | "rightbracket" => Ok(30),
        "'" | "quote" => Ok(39),
        ";" | "semicolon" => Ok(41),
        "\\" | "backslash" => Ok(42),
        "," | "comma" => Ok(43),
        "/" | "slash" => Ok(44),
        "." | "period" => Ok(47),
        "`" | "grave" => Ok(50),

        _ => Err(InteractionError::UnknownKey {
            name: name.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_key_enter() {
        let mapping = parse_key_combo("enter").unwrap();
        assert_eq!(mapping.keycode, 36);
        assert_eq!(mapping.flags, CGEventFlags::CGEventFlagNull);
    }

    #[test]
    fn test_parse_single_key_tab() {
        let mapping = parse_key_combo("tab").unwrap();
        assert_eq!(mapping.keycode, 48);
        assert_eq!(mapping.flags, CGEventFlags::CGEventFlagNull);
    }

    #[test]
    fn test_parse_single_key_escape() {
        let mapping = parse_key_combo("escape").unwrap();
        assert_eq!(mapping.keycode, 53);
        assert_eq!(mapping.flags, CGEventFlags::CGEventFlagNull);
    }

    #[test]
    fn test_parse_single_key_space() {
        let mapping = parse_key_combo("space").unwrap();
        assert_eq!(mapping.keycode, 49);
        assert_eq!(mapping.flags, CGEventFlags::CGEventFlagNull);
    }

    #[test]
    fn test_parse_cmd_s() {
        let mapping = parse_key_combo("cmd+s").unwrap();
        assert_eq!(mapping.keycode, 1); // 's' keycode
        assert!(mapping.flags.contains(CGEventFlags::CGEventFlagCommand));
    }

    #[test]
    fn test_parse_cmd_shift_p() {
        let mapping = parse_key_combo("cmd+shift+p").unwrap();
        assert_eq!(mapping.keycode, 35); // 'p' keycode
        assert!(mapping.flags.contains(CGEventFlags::CGEventFlagCommand));
        assert!(mapping.flags.contains(CGEventFlags::CGEventFlagShift));
    }

    #[test]
    fn test_parse_ctrl_c() {
        let mapping = parse_key_combo("ctrl+c").unwrap();
        assert_eq!(mapping.keycode, 8); // 'c' keycode
        assert!(mapping.flags.contains(CGEventFlags::CGEventFlagControl));
    }

    #[test]
    fn test_parse_alt_tab() {
        let mapping = parse_key_combo("alt+tab").unwrap();
        assert_eq!(mapping.keycode, 48); // tab keycode
        assert!(mapping.flags.contains(CGEventFlags::CGEventFlagAlternate));
    }

    #[test]
    fn test_parse_case_insensitive() {
        let mapping = parse_key_combo("CMD+S").unwrap();
        assert_eq!(mapping.keycode, 1);
        assert!(mapping.flags.contains(CGEventFlags::CGEventFlagCommand));
    }

    #[test]
    fn test_parse_command_alias() {
        let mapping = parse_key_combo("command+s").unwrap();
        assert_eq!(mapping.keycode, 1);
        assert!(mapping.flags.contains(CGEventFlags::CGEventFlagCommand));
    }

    #[test]
    fn test_parse_control_alias() {
        let mapping = parse_key_combo("control+c").unwrap();
        assert_eq!(mapping.keycode, 8);
        assert!(mapping.flags.contains(CGEventFlags::CGEventFlagControl));
    }

    #[test]
    fn test_parse_option_alias() {
        let mapping = parse_key_combo("option+a").unwrap();
        assert_eq!(mapping.keycode, 0);
        assert!(mapping.flags.contains(CGEventFlags::CGEventFlagAlternate));
    }

    #[test]
    fn test_parse_opt_alias() {
        let mapping = parse_key_combo("opt+a").unwrap();
        assert_eq!(mapping.keycode, 0);
        assert!(mapping.flags.contains(CGEventFlags::CGEventFlagAlternate));
    }

    #[test]
    fn test_parse_unknown_key() {
        let result = parse_key_combo("unknownkey");
        assert!(result.is_err());
        if let Err(InteractionError::UnknownKey { name }) = result {
            assert_eq!(name, "unknownkey");
        } else {
            panic!("Expected UnknownKey error");
        }
    }

    #[test]
    fn test_parse_arrow_keys() {
        assert_eq!(parse_key_combo("up").unwrap().keycode, 126);
        assert_eq!(parse_key_combo("down").unwrap().keycode, 125);
        assert_eq!(parse_key_combo("left").unwrap().keycode, 123);
        assert_eq!(parse_key_combo("right").unwrap().keycode, 124);
    }

    #[test]
    fn test_parse_function_keys() {
        assert_eq!(parse_key_combo("f1").unwrap().keycode, 122);
        assert_eq!(parse_key_combo("f5").unwrap().keycode, 96);
        assert_eq!(parse_key_combo("f12").unwrap().keycode, 111);
    }

    #[test]
    fn test_parse_return_alias() {
        let mapping = parse_key_combo("return").unwrap();
        assert_eq!(mapping.keycode, 36);
    }

    #[test]
    fn test_parse_esc_alias() {
        let mapping = parse_key_combo("esc").unwrap();
        assert_eq!(mapping.keycode, 53);
    }

    #[test]
    fn test_parse_backspace_alias() {
        let mapping = parse_key_combo("backspace").unwrap();
        assert_eq!(mapping.keycode, 51);
    }

    #[test]
    fn test_parse_delete_key() {
        let mapping = parse_key_combo("delete").unwrap();
        assert_eq!(mapping.keycode, 51);
    }

    #[test]
    fn test_parse_number_keys() {
        assert_eq!(parse_key_combo("0").unwrap().keycode, 29);
        assert_eq!(parse_key_combo("1").unwrap().keycode, 18);
        assert_eq!(parse_key_combo("9").unwrap().keycode, 25);
    }

    #[test]
    fn test_parse_all_modifiers() {
        let mapping = parse_key_combo("cmd+shift+ctrl+alt+a").unwrap();
        assert_eq!(mapping.keycode, 0);
        assert!(mapping.flags.contains(CGEventFlags::CGEventFlagCommand));
        assert!(mapping.flags.contains(CGEventFlags::CGEventFlagShift));
        assert!(mapping.flags.contains(CGEventFlags::CGEventFlagControl));
        assert!(mapping.flags.contains(CGEventFlags::CGEventFlagAlternate));
    }

    #[test]
    fn test_parse_modifiers_only_is_error() {
        let result = parse_key_combo("cmd+shift");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_empty_is_error() {
        let result = parse_key_combo("");
        assert!(result.is_err());
    }
}
