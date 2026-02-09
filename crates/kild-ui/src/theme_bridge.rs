//! Bridge between KILD's Tallinn Night theme and gpui-component's theme system.
//!
//! Maps our brand colors to gpui-component theme tokens so that
//! library components (Button, Dialog, Input) render with Tallinn Night colors.

use std::rc::Rc;

use gpui::{App, SharedString};
use gpui_component::theme::{Theme, ThemeConfig, ThemeConfigColors, ThemeMode};

/// Apply the Tallinn Night dark theme to gpui-component's global theme.
pub fn apply_tallinn_night_theme(cx: &mut App) {
    let config = Rc::new(ThemeConfig {
        name: SharedString::from("Tallinn Night"),
        mode: ThemeMode::Dark,
        is_default: true,
        font_family: Some("Inter".into()),
        mono_font_family: Some("JetBrains Mono".into()),
        font_size: Some(13.0),
        mono_font_size: Some(13.0),
        radius: Some(6),
        radius_lg: Some(8),
        shadow: Some(true),
        colors: tallinn_night_colors(),
        highlight: None,
    });

    Theme::global_mut(cx).apply_config(&config);
}

fn tallinn_night_colors() -> ThemeConfigColors {
    // ThemeConfigColors has private base color fields, so we deserialize from JSON
    // to construct it rather than using struct literal syntax.
    //
    // Hover colors (success.hover, danger.hover, warning.hover) are lightened
    // variants for gpui-component button states, not part of KILD's base palette.
    //
    // Compatible with gpui-component 0.5.1. Verify theme keys if upgrading.
    let json = r##"{
        "background": "#0E1012",
        "foreground": "#B8C0CC",
        "border": "#2D3139",
        "input.border": "#2D3139",
        "accent.background": "#1C1F22",
        "accent.foreground": "#E8ECF0",
        "primary.background": "#7CB4C8",
        "primary.hover.background": "#98CCE0",
        "primary.active.background": "#5E9AB2",
        "primary.foreground": "#F8FAFC",
        "secondary.background": "#1C1F22",
        "secondary.hover.background": "#2D3139",
        "secondary.active.background": "#151719",
        "secondary.foreground": "#B8C0CC",
        "success.background": "#6B8F5E",
        "success.hover.background": "#7DA870",
        "success.active.background": "#556F4A",
        "success.foreground": "#F8FAFC",
        "danger.background": "#B87060",
        "danger.hover.background": "#CC8878",
        "danger.active.background": "#9A584A",
        "danger.foreground": "#F8FAFC",
        "warning.background": "#C49A5C",
        "warning.hover.background": "#D4AD72",
        "warning.active.background": "#A07D45",
        "warning.foreground": "#0E1012",
        "info.background": "#5E9AB2",
        "info.hover.background": "#7CB4C8",
        "info.active.background": "#4A8298",
        "info.foreground": "#F8FAFC",
        "muted.background": "#1C1F22",
        "muted.foreground": "#5C6370",
        "ring": "#7CB4C8",
        "overlay": "#08090ACC",
        "popover.background": "#1C1F22",
        "popover.foreground": "#B8C0CC",
        "sidebar.background": "#0E1012",
        "sidebar.foreground": "#B8C0CC",
        "sidebar.accent.background": "#1C1F22",
        "sidebar.accent.foreground": "#E8ECF0",
        "sidebar.primary.background": "#7CB4C8",
        "sidebar.primary.foreground": "#F8FAFC",
        "sidebar.border": "#1F2328",
        "list.background": "#0E1012",
        "list.hover.background": "#1C1F22",
        "list.active.background": "#7CB4C833",
        "list.active.border": "#7CB4C8",
        "list.even.background": "#151719",
        "list.head.background": "#0E1012",
        "tab.background": "#0E1012",
        "tab.active.background": "#151719",
        "tab.active.foreground": "#E8ECF0",
        "tab.foreground": "#5C6370",
        "tab_bar.background": "#08090A",
        "table.background": "#0E1012",
        "table.hover.background": "#1C1F22",
        "table.active.background": "#7CB4C833",
        "table.active.border": "#7CB4C8",
        "table.even.background": "#151719",
        "table.head.background": "#08090A",
        "table.head.foreground": "#848D9C",
        "table.row.border": "#1F2328",
        "scrollbar.background": "#0E1012",
        "scrollbar.thumb.background": "#2D3139",
        "scrollbar.thumb.hover.background": "#3D434D",
        "selection.background": "#7CB4C833",
        "caret": "#7CB4C8",
        "title_bar.background": "#08090A",
        "title_bar.border": "#1F2328",
        "window.border": "#1F2328",
        "link": "#7CB4C8",
        "link.hover": "#98CCE0",
        "link.active": "#5E9AB2",
        "skeleton.background": "#1C1F22",
        "progress.bar.background": "#7CB4C8",
        "drag.border": "#7CB4C8",
        "drop_target.background": "#7CB4C822"
    }"##;
    match serde_json::from_str(json) {
        Ok(colors) => colors,
        Err(e) => {
            tracing::error!(
                event = "ui.theme.deserialization_failed",
                error = %e,
                "Tallinn Night theme JSON is invalid — falling back to library defaults. \
                 This is a bug: the JSON schema may be incompatible with this gpui-component version."
            );
            ThemeConfigColors::default()
        }
    }
}
