//! Button component with themed variants.
//!
//! Provides consistent button styling across the application.
//! All colors come from the theme module.

use gpui::{
    ClickEvent, ElementId, IntoElement, RenderOnce, Rgba, SharedString, Window, div, prelude::*, px,
};

use crate::theme;

/// Click handler type for buttons.
type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static>;

/// Button style variants matching the brand system.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    /// Primary action - Ice background, used for main CTAs
    #[default]
    Primary,
    /// Secondary action - Surface background with border
    Secondary,
    /// Ghost button - Transparent background, surface background on hover
    Ghost,
    /// Success action - Aurora (green) background
    Success,
    /// Warning action - Copper (amber) background
    Warning,
    /// Danger action - Transparent with Ember (red) text and border
    Danger,
}

impl ButtonVariant {
    /// Get the background color for this variant.
    fn bg_color(&self, disabled: bool) -> Rgba {
        if disabled {
            return theme::surface();
        }
        match self {
            ButtonVariant::Primary => theme::ice(),
            ButtonVariant::Secondary => theme::surface(),
            ButtonVariant::Ghost => theme::with_alpha(theme::void(), 0.0),
            ButtonVariant::Success => theme::aurora(),
            ButtonVariant::Warning => theme::copper(),
            ButtonVariant::Danger => theme::with_alpha(theme::void(), 0.0),
        }
    }

    /// Get the hover background color for this variant.
    fn hover_color(&self) -> Rgba {
        match self {
            ButtonVariant::Primary => theme::ice_bright(),
            ButtonVariant::Secondary => theme::elevated(),
            ButtonVariant::Ghost => theme::surface(),
            ButtonVariant::Success => theme::aurora_dim(),
            ButtonVariant::Warning => theme::copper_dim(),
            ButtonVariant::Danger => theme::with_alpha(theme::ember(), 0.15),
        }
    }

    /// Get the text color for this variant.
    fn text_color(&self, disabled: bool) -> Rgba {
        if disabled {
            return theme::text_muted();
        }
        match self {
            ButtonVariant::Primary => theme::void(),
            ButtonVariant::Secondary => theme::text(),
            ButtonVariant::Ghost => theme::text_subtle(),
            ButtonVariant::Success => theme::void(),
            ButtonVariant::Warning => theme::void(),
            ButtonVariant::Danger => theme::ember(),
        }
    }

    /// Get the border color for this variant.
    ///
    /// Returns transparent for variants without borders (Primary, Ghost, Success, Warning).
    /// Secondary and Danger variants have visible borders.
    fn border_color(&self) -> Rgba {
        match self {
            ButtonVariant::Secondary => theme::border(),
            ButtonVariant::Danger => theme::ember(),
            _ => theme::with_alpha(theme::void(), 0.0),
        }
    }
}

/// A styled button component.
///
/// # Example
///
/// ```ignore
/// Button::new("create-btn", "Create")
///     .variant(ButtonVariant::Primary)
///     .on_click(cx.listener(|view, _, _, cx| {
///         view.on_create(cx);
///     }))
/// ```
#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
    variant: ButtonVariant,
    disabled: bool,
    on_click: Option<ClickHandler>,
}

impl Button {
    /// Create a new button with the given ID and label.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            variant: ButtonVariant::default(),
            disabled: false,
            on_click: None,
        }
    }

    /// Set the button variant (styling).
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Set whether the button is disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Set the click handler.
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _window: &mut Window, _cx: &mut gpui::App) -> impl IntoElement {
        let bg = self.variant.bg_color(self.disabled);
        let hover_bg = self.variant.hover_color();
        let text = self.variant.text_color(self.disabled);
        let border = self.variant.border_color();
        let on_click = self.on_click;

        let mut button = div()
            .id(self.id)
            .px(px(theme::SPACE_3))
            .py(px(theme::SPACE_2))
            .bg(bg)
            .border_1()
            .border_color(border)
            .rounded(px(theme::RADIUS_MD))
            .child(div().text_color(text).child(self.label));

        if self.disabled {
            // Disabled: show not-allowed cursor to indicate non-interactivity
            button = button.cursor(gpui::CursorStyle::OperationNotAllowed);
        } else if let Some(handler) = on_click {
            // Enabled with handler: interactive button with hover and click
            button = button
                .hover(|style| style.bg(hover_bg))
                .cursor_pointer()
                .on_click(handler);
        }
        // Enabled without handler: no hover/cursor changes (display-only button)

        button
    }
}
