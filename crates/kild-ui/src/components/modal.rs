//! Modal component for dialog overlays.
//!
//! Provides consistent modal styling with overlay, centered dialog box,
//! and header/body/footer structure. All colors come from the theme module.

use gpui::{
    AnyElement, ElementId, IntoElement, Pixels, RenderOnce, SharedString, Window, div, prelude::*,
    px,
};

use crate::theme;

/// Default modal width (400px, matching existing dialogs).
const DEFAULT_WIDTH: f32 = 400.0;

/// A styled modal dialog component.
///
/// Modal renders:
/// - Semi-transparent overlay covering the screen
/// - Centered dialog box with themed styling
/// - Header with title and bottom border
/// - Body content area
/// - Optional footer with top border (typically for buttons)
///
/// # Examples
///
/// ## Action dialog with footer
///
/// ```ignore
/// use gpui::{div, px, prelude::*};
/// use crate::components::{Modal, Button, ButtonVariant};
///
/// Modal::new("create-dialog", "Create New KILD")
///     .body(
///         div().flex_col().gap_4()
///             .child(/* form fields */)
///     )
///     .footer(
///         div().flex().justify_end().gap_2()
///             .child(Button::new("cancel", "Cancel").variant(ButtonVariant::Secondary))
///             .child(Button::new("create", "Create").variant(ButtonVariant::Primary))
///     )
/// ```
///
/// ## Informational dialog without footer
///
/// ```ignore
/// Modal::new("info-dialog", "Information")
///     .body(div().child("This is a read-only message."))
/// ```
///
/// ## Custom width
///
/// ```ignore
/// Modal::new("settings-dialog", "Settings")
///     .width(px(600.0))
///     .body(/* wide content */)
///     .footer(/* buttons */)
/// ```
#[derive(IntoElement)]
pub struct Modal {
    /// Base ID used to generate unique element IDs for overlay and dialog box.
    id: SharedString,
    title: SharedString,
    body: Option<AnyElement>,
    footer: Option<AnyElement>,
    width: Pixels,
}

impl Modal {
    /// Create a new modal with the given ID and title.
    ///
    /// The ID is used as a base to generate unique element IDs:
    /// - `{id}` for the overlay
    /// - `{id}-box` for the dialog box
    pub fn new(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            body: None,
            footer: None,
            width: px(DEFAULT_WIDTH),
        }
    }

    /// Set the body content of the modal.
    pub fn body(mut self, body: impl IntoElement) -> Self {
        self.body = Some(body.into_any_element());
        self
    }

    /// Set the footer content of the modal (typically buttons).
    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    /// Set a custom width for the modal (default: 400px).
    pub fn width(mut self, width: impl Into<Pixels>) -> Self {
        self.width = width.into();
        self
    }
}

impl RenderOnce for Modal {
    fn render(self, _window: &mut Window, _cx: &mut gpui::App) -> impl IntoElement {
        // Overlay: covers entire screen with semi-transparent background
        div()
            .id(ElementId::Name(self.id.clone()))
            .absolute()
            .inset_0()
            .bg(theme::overlay())
            .flex()
            .justify_center()
            .items_center()
            // Dialog box: centered, themed container
            .child(
                div()
                    .id(ElementId::Name(format!("{}-box", self.id).into()))
                    .w(self.width)
                    .bg(theme::elevated())
                    .rounded(px(theme::RADIUS_LG))
                    .border_1()
                    .border_color(theme::border())
                    .flex()
                    .flex_col()
                    // Header: title with bottom border
                    .child(
                        div()
                            .px(px(theme::SPACE_4))
                            .py(px(theme::SPACE_3))
                            .border_b_1()
                            .border_color(theme::border_subtle())
                            .child(
                                div()
                                    .text_size(px(theme::TEXT_LG))
                                    .text_color(theme::text_bright())
                                    .child(self.title),
                            ),
                    )
                    // Body: main content area
                    .when_some(self.body, |this, body| {
                        this.child(
                            div()
                                .px(px(theme::SPACE_4))
                                .py(px(theme::SPACE_4))
                                .child(body),
                        )
                    })
                    // Footer: optional, typically for action buttons
                    .when_some(self.footer, |this, footer| {
                        this.child(
                            div()
                                .px(px(theme::SPACE_4))
                                .py(px(theme::SPACE_3))
                                .border_t_1()
                                .border_color(theme::border_subtle())
                                .child(footer),
                        )
                    }),
            )
    }
}
