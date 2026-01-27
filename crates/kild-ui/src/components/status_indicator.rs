//! Status indicator component for kild session states.
//!
//! Provides consistent status visualization with dots and badges.
//! All colors come from the theme module (Tallinn Night brand).

// Allow dead_code - this component is defined ahead of usage in view components.
// Remove this attribute once views are migrated to use StatusIndicator (Phase 9.6).
#![allow(dead_code)]

use gpui::{IntoElement, RenderOnce, Rgba, Window, div, prelude::*, px};

use crate::theme;

/// Visual status states for kilds.
///
/// Maps to ProcessStatus but with visual-focused naming.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Status {
    /// Running/active - Aurora (green) with glow
    #[default]
    Active,
    /// Stopped/idle - Copper (amber) no glow
    Stopped,
    /// Crashed/error - Ember (red) with glow
    Crashed,
}

impl Status {
    /// Get the primary color for this status.
    pub fn color(&self) -> Rgba {
        match self {
            Status::Active => theme::aurora(),
            Status::Stopped => theme::copper(),
            Status::Crashed => theme::ember(),
        }
    }

    /// Get the glow/background color for this status (15% alpha).
    ///
    /// Returns `None` for statuses that shouldn't have a glow effect (Stopped).
    /// This enforces the "Stopped has no glow" invariant at the type level.
    pub fn glow_color(&self) -> Option<Rgba> {
        match self {
            Status::Active | Status::Crashed => Some(theme::with_alpha(self.color(), 0.15)),
            Status::Stopped => None,
        }
    }

    /// Get the text label for badge display.
    pub fn label(&self) -> &'static str {
        match self {
            Status::Active => "Active",
            Status::Stopped => "Stopped",
            Status::Crashed => "Crashed",
        }
    }
}

/// Display mode for the status indicator.
///
/// This represents the structural display variant, not just size.
/// - `Dot`: Simple 8px circle indicator
/// - `Badge`: Pill-shaped container with dot and text label
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StatusMode {
    /// Small dot (8px circle)
    #[default]
    Dot,
    /// Badge with text (pill shape with dot + label)
    Badge,
}

/// A status indicator component.
///
/// # Example
///
/// ```ignore
/// // Simple dot indicator
/// StatusIndicator::dot(Status::Active)
///
/// // Badge with label
/// StatusIndicator::badge(Status::Stopped)
/// ```
#[derive(IntoElement)]
pub struct StatusIndicator {
    status: Status,
    mode: StatusMode,
}

impl StatusIndicator {
    /// Create a dot indicator (8px colored circle).
    pub fn dot(status: Status) -> Self {
        Self {
            status,
            mode: StatusMode::Dot,
        }
    }

    /// Create a badge indicator (pill with dot + text label).
    pub fn badge(status: Status) -> Self {
        Self {
            status,
            mode: StatusMode::Badge,
        }
    }
}

impl RenderOnce for StatusIndicator {
    fn render(self, _window: &mut Window, _cx: &mut gpui::App) -> impl IntoElement {
        let color = self.status.color();
        let glow = self.status.glow_color();

        match self.mode {
            StatusMode::Dot => {
                // 8px circle with optional glow
                let dot = div().size(px(8.0)).rounded_full().bg(color);

                // Add glow effect via larger background container
                // GPUI doesn't have box-shadow, so we simulate with a background element
                if let Some(glow_color) = glow {
                    // For glow, wrap in a container with glow background
                    div()
                        .size(px(16.0))
                        .rounded_full()
                        .bg(glow_color)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(dot)
                        .into_any_element()
                } else {
                    dot.into_any_element()
                }
            }
            StatusMode::Badge => {
                // Pill shape: background glow + dot + text
                // For badges without glow, use transparent background
                let bg_color = glow.unwrap_or(Rgba {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                });
                div()
                    .flex()
                    .items_center()
                    .gap(px(theme::SPACE_1))
                    .px(px(theme::SPACE_2))
                    .py(px(2.0))
                    .bg(bg_color)
                    .rounded(px(theme::RADIUS_SM))
                    .child(
                        // Small dot inside badge (6px for visual balance)
                        div().size(px(6.0)).rounded_full().bg(color),
                    )
                    .child(
                        div()
                            .text_color(color)
                            .text_size(px(theme::TEXT_XS))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(self.status.label()),
                    )
                    .into_any_element()
            }
        }
    }
}
