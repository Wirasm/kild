//! Theme constants for KILD UI.
//!
//! Color palette based on the "Tallinn Night" brand system.
//! All colors are GPUI Rgba values for direct use in styling.
//!
//! # Usage
//!
//! ```ignore
//! use crate::theme;
//!
//! div()
//!     .bg(theme::surface())
//!     .text_color(theme::text_bright())
//!     .border_color(theme::ice())
//! ```

// Allow unused items - these are part of the complete brand system
// and will be used as the UI expands. Better to keep the full palette
// defined than to remove and re-add later.
#![allow(dead_code)]

use gpui::Rgba;

// =============================================================================
// COLOR PALETTE - Tallinn Night (Dark Theme)
// =============================================================================

// Base surfaces (darkest to lightest)
// - void: deepest background, app edges, behind everything
// - obsidian: sidebars, panels
// - surface: cards, content areas
// - elevated: modals, dropdowns, floating elements
pub fn void() -> Rgba {
    gpui::rgb(0x08090A)
}
pub fn obsidian() -> Rgba {
    gpui::rgb(0x0E1012)
}
pub fn surface() -> Rgba {
    gpui::rgb(0x151719)
}
pub fn elevated() -> Rgba {
    gpui::rgb(0x1C1F22)
}

// Borders (subtle to strong)
pub fn border_subtle() -> Rgba {
    gpui::rgb(0x1F2328)
}
pub fn border() -> Rgba {
    gpui::rgb(0x2D3139)
}
pub fn border_strong() -> Rgba {
    gpui::rgb(0x3D434D)
}

// Text (muted to brightest)
pub fn text_muted() -> Rgba {
    gpui::rgb(0x5C6370)
}
pub fn text_subtle() -> Rgba {
    gpui::rgb(0x848D9C)
}
pub fn text() -> Rgba {
    gpui::rgb(0xB8C0CC)
}
pub fn text_bright() -> Rgba {
    gpui::rgb(0xE8ECF0)
}
pub fn text_white() -> Rgba {
    gpui::rgb(0xF8FAFC)
}

// Primary accent - Ice (for primary actions, focus states)
pub fn ice() -> Rgba {
    gpui::rgb(0x38BDF8)
}
pub fn ice_dim() -> Rgba {
    gpui::rgb(0x0EA5E9)
}
pub fn ice_bright() -> Rgba {
    gpui::rgb(0x7DD3FC)
}

// Status - Aurora (active/running/success)
pub fn aurora() -> Rgba {
    gpui::rgb(0x34D399)
}
pub fn aurora_dim() -> Rgba {
    gpui::rgb(0x10B981)
}

// Status - Copper (stopped/warning/idle)
pub fn copper() -> Rgba {
    gpui::rgb(0xFBBF24)
}
pub fn copper_dim() -> Rgba {
    gpui::rgb(0xD97706)
}

// Status - Ember (error/crashed/danger)
pub fn ember() -> Rgba {
    gpui::rgb(0xF87171)
}

// Agent indicator - Kiri (purple, for AI activity)
pub fn kiri() -> Rgba {
    gpui::rgb(0xA78BFA)
}

// Secondary accent - Blade (for secondary actions)
pub fn blade() -> Rgba {
    gpui::rgb(0x64748B)
}
pub fn blade_bright() -> Rgba {
    gpui::rgb(0x94A3B8)
}

// =============================================================================
// GLOW EFFECTS (colors with alpha for shadows/glows)
// =============================================================================

/// Standard glow effect alpha (15% opacity).
///
/// Used consistently for all glow effects (status indicators, danger hover, etc.)
pub const GLOW_ALPHA: f32 = 0.15;

/// Create a color with alpha for glow effects.
///
/// Alpha is clamped to the valid range 0.0-1.0.
///
/// # Example
/// ```ignore
/// // For glow effects, use GLOW_ALPHA:
/// let ice_glow = with_alpha(ice(), GLOW_ALPHA);
/// ```
pub fn with_alpha(color: Rgba, alpha: f32) -> Rgba {
    Rgba {
        a: alpha.clamp(0.0, 1.0),
        ..color
    }
}

/// Fully transparent color (alpha = 0.0).
///
/// Use for variants that should appear borderless or backgroundless.
pub fn transparent() -> Rgba {
    Rgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    }
}

// =============================================================================
// TYPOGRAPHY SCALE
// =============================================================================

pub const TEXT_XS: f32 = 11.0;
pub const TEXT_SM: f32 = 12.0;
pub const TEXT_BASE: f32 = 13.0;
pub const TEXT_MD: f32 = 14.0;
pub const TEXT_LG: f32 = 16.0;
pub const TEXT_XL: f32 = 18.0;

// Font families (for reference - actual fonts set at app level)
pub const FONT_UI: &str = "Inter";
pub const FONT_MONO: &str = "JetBrains Mono";

// =============================================================================
// SPACING SCALE
// =============================================================================

pub const SPACE_1: f32 = 4.0;
pub const SPACE_2: f32 = 8.0;
pub const SPACE_3: f32 = 12.0;
pub const SPACE_4: f32 = 16.0;
pub const SPACE_5: f32 = 20.0;
pub const SPACE_6: f32 = 24.0;

// =============================================================================
// BORDER RADII
// =============================================================================

pub const RADIUS_SM: f32 = 4.0;
pub const RADIUS_MD: f32 = 6.0;
pub const RADIUS_LG: f32 = 8.0;

// =============================================================================
// COMPONENT SIZES
// =============================================================================

/// Status indicator dot size (simple dot mode).
pub const STATUS_DOT_SIZE: f32 = 8.0;

/// Status indicator glow container size (wraps the dot for glow effect).
pub const STATUS_GLOW_SIZE: f32 = 16.0;

/// Status indicator badge dot size (smaller dot inside badge).
pub const STATUS_BADGE_DOT_SIZE: f32 = 6.0;

// =============================================================================
// OVERLAY
// =============================================================================

/// Semi-transparent overlay for modals (Void at 80% opacity)
pub fn overlay() -> Rgba {
    gpui::rgba(0x08090ACC)
}
