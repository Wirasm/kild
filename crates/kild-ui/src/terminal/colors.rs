use alacritty_terminal::vte::ansi::{Color, NamedColor};
use gpui::{Hsla, Rgba};

use crate::theme;

/// ANSI 16 color lookup table mapped to Tallinn Night palette.
///
/// Index 0-7: standard colors, 8-15: bright colors.
/// These are intentionally more vivid than the muted brand palette
/// for terminal readability on dark backgrounds.
fn ansi_colors() -> [Hsla; 16] {
    [
        Hsla::from(theme::ansi_black()),
        Hsla::from(theme::ansi_red()),
        Hsla::from(theme::ansi_green()),
        Hsla::from(theme::ansi_yellow()),
        Hsla::from(theme::ansi_blue()),
        Hsla::from(theme::ansi_magenta()),
        Hsla::from(theme::ansi_cyan()),
        Hsla::from(theme::ansi_white()),
        Hsla::from(theme::ansi_bright_black()),
        Hsla::from(theme::ansi_bright_red()),
        Hsla::from(theme::ansi_bright_green()),
        Hsla::from(theme::ansi_bright_yellow()),
        Hsla::from(theme::ansi_bright_blue()),
        Hsla::from(theme::ansi_bright_magenta()),
        Hsla::from(theme::ansi_bright_cyan()),
        Hsla::from(theme::ansi_bright_white()),
    ]
}

/// Convert an alacritty_terminal Color to a GPUI Hsla.
pub fn resolve_color(color: &Color) -> Hsla {
    match color {
        Color::Named(named) => resolve_named(*named),
        Color::Indexed(idx) => resolve_indexed(*idx),
        Color::Spec(rgb) => Hsla::from(Rgba {
            r: rgb.r as f32 / 255.0,
            g: rgb.g as f32 / 255.0,
            b: rgb.b as f32 / 255.0,
            a: 1.0,
        }),
    }
}

/// Resolve a named ANSI color to Hsla.
fn resolve_named(named: NamedColor) -> Hsla {
    let table = ansi_colors();
    match named {
        // Standard 0-7
        NamedColor::Black => table[0],
        NamedColor::Red => table[1],
        NamedColor::Green => table[2],
        NamedColor::Yellow => table[3],
        NamedColor::Blue => table[4],
        NamedColor::Magenta => table[5],
        NamedColor::Cyan => table[6],
        NamedColor::White => table[7],
        // Bright 8-15
        NamedColor::BrightBlack => table[8],
        NamedColor::BrightRed => table[9],
        NamedColor::BrightGreen => table[10],
        NamedColor::BrightYellow => table[11],
        NamedColor::BrightBlue => table[12],
        NamedColor::BrightMagenta => table[13],
        NamedColor::BrightCyan => table[14],
        NamedColor::BrightWhite => table[15],
        // Dim variants — use standard colors at reduced lightness
        NamedColor::DimBlack => dim(table[0]),
        NamedColor::DimRed => dim(table[1]),
        NamedColor::DimGreen => dim(table[2]),
        NamedColor::DimYellow => dim(table[3]),
        NamedColor::DimBlue => dim(table[4]),
        NamedColor::DimMagenta => dim(table[5]),
        NamedColor::DimCyan => dim(table[6]),
        NamedColor::DimWhite => dim(table[7]),
        // Special colors
        NamedColor::Foreground | NamedColor::BrightForeground => {
            Hsla::from(theme::terminal_foreground())
        }
        NamedColor::Background => Hsla::from(theme::terminal_background()),
        NamedColor::Cursor => Hsla::from(theme::terminal_cursor()),
        NamedColor::DimForeground => dim(Hsla::from(theme::terminal_foreground())),
    }
}

/// Resolve an indexed color (0-255) to Hsla.
fn resolve_indexed(idx: u8) -> Hsla {
    match idx {
        // 0-15: same as named ANSI colors
        0..=15 => {
            let table = ansi_colors();
            table[idx as usize]
        }
        // 16-231: 6x6x6 RGB color cube
        16..=231 => {
            let idx = idx - 16;
            let r = idx / 36;
            let g = (idx % 36) / 6;
            let b = idx % 6;
            let to_component = |c: u8| -> f32 {
                if c == 0 {
                    0.0
                } else {
                    (55.0 + 40.0 * c as f32) / 255.0
                }
            };
            Hsla::from(Rgba {
                r: to_component(r),
                g: to_component(g),
                b: to_component(b),
                a: 1.0,
            })
        }
        // 232-255: 24-step grayscale ramp
        232..=255 => {
            let value = (8.0 + 10.0 * (idx - 232) as f32) / 255.0;
            Hsla::from(Rgba {
                r: value,
                g: value,
                b: value,
                a: 1.0,
            })
        }
    }
}

/// Reduce lightness to 67% for dim variants.
fn dim(color: Hsla) -> Hsla {
    Hsla {
        l: color.l * 0.67,
        ..color
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_named_standard_colors_resolve() {
        let black = resolve_color(&Color::Named(NamedColor::Black));
        let red = resolve_color(&Color::Named(NamedColor::Red));
        let white = resolve_color(&Color::Named(NamedColor::White));

        // All should have full alpha
        assert!((black.a - 1.0).abs() < f32::EPSILON);
        assert!((red.a - 1.0).abs() < f32::EPSILON);
        assert!((white.a - 1.0).abs() < f32::EPSILON);

        // Red should be more vivid (higher saturation) than black
        assert!(red.s > black.s);
    }

    #[test]
    fn test_named_bright_colors_resolve() {
        let bright_red = resolve_color(&Color::Named(NamedColor::BrightRed));
        let red = resolve_color(&Color::Named(NamedColor::Red));

        // Bright red should be lighter than standard red
        assert!(bright_red.l > red.l);
    }

    #[test]
    fn test_named_dim_colors_are_darker() {
        let red = resolve_color(&Color::Named(NamedColor::Red));
        let dim_red = resolve_color(&Color::Named(NamedColor::DimRed));

        // Dim should have lower lightness
        assert!(dim_red.l < red.l);
    }

    #[test]
    fn test_named_special_colors() {
        let fg = resolve_color(&Color::Named(NamedColor::Foreground));
        let bg = resolve_color(&Color::Named(NamedColor::Background));
        let cursor = resolve_color(&Color::Named(NamedColor::Cursor));

        assert!((fg.a - 1.0).abs() < f32::EPSILON);
        assert!((bg.a - 1.0).abs() < f32::EPSILON);
        assert!((cursor.a - 1.0).abs() < f32::EPSILON);

        // Background should be very dark (low lightness)
        assert!(bg.l < 0.1);
        // Foreground should be lighter than background
        assert!(fg.l > bg.l);
    }

    #[test]
    fn test_indexed_0_to_15_matches_named() {
        let named_red = resolve_color(&Color::Named(NamedColor::Red));
        let indexed_red = resolve_color(&Color::Indexed(1));

        assert!((named_red.h - indexed_red.h).abs() < f32::EPSILON);
        assert!((named_red.s - indexed_red.s).abs() < f32::EPSILON);
        assert!((named_red.l - indexed_red.l).abs() < f32::EPSILON);
    }

    #[test]
    fn test_indexed_rgb_cube() {
        // Index 16 = r=0, g=0, b=0 = black
        let black = resolve_color(&Color::Indexed(16));
        assert!(black.l < 0.01);

        // Index 231 = r=5, g=5, b=5 = near-white
        let white = resolve_color(&Color::Indexed(231));
        assert!(white.l > 0.9);

        // Index 196 = r=5, g=0, b=0 = pure red
        let red = resolve_color(&Color::Indexed(196));
        assert!(red.s > 0.5); // Should be saturated
    }

    #[test]
    fn test_indexed_grayscale_ramp() {
        let dark = resolve_color(&Color::Indexed(232));
        let light = resolve_color(&Color::Indexed(255));

        // Dark end should be darker than light end
        assert!(dark.l < light.l);
        // Both should have zero or near-zero saturation (grayscale)
        assert!(dark.s < 0.01);
        assert!(light.s < 0.01);
    }

    #[test]
    fn test_spec_rgb_direct() {
        let color = resolve_color(&Color::Spec(alacritty_terminal::vte::ansi::Rgb {
            r: 255,
            g: 0,
            b: 0,
        }));

        // Pure red: should have high saturation, full alpha
        assert!(color.s > 0.9);
        assert!((color.a - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_spec_rgb_white() {
        let color = resolve_color(&Color::Spec(alacritty_terminal::vte::ansi::Rgb {
            r: 255,
            g: 255,
            b: 255,
        }));

        assert!(color.l > 0.99);
    }

    #[test]
    fn test_all_16_named_colors_have_full_alpha() {
        let named_colors = [
            NamedColor::Black,
            NamedColor::Red,
            NamedColor::Green,
            NamedColor::Yellow,
            NamedColor::Blue,
            NamedColor::Magenta,
            NamedColor::Cyan,
            NamedColor::White,
            NamedColor::BrightBlack,
            NamedColor::BrightRed,
            NamedColor::BrightGreen,
            NamedColor::BrightYellow,
            NamedColor::BrightBlue,
            NamedColor::BrightMagenta,
            NamedColor::BrightCyan,
            NamedColor::BrightWhite,
        ];

        for named in named_colors {
            let color = resolve_color(&Color::Named(named));
            assert!(
                (color.a - 1.0).abs() < f32::EPSILON,
                "{named:?} should have alpha 1.0, got {}",
                color.a
            );
        }
    }
}
