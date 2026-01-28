//! TextInput component with themed styling.
//!
//! A display-only text input that renders value, placeholder, and cursor.
//! This component does not handle keyboard events or maintain input state -
//! the parent view is responsible for event handling and state management.

use gpui::{ElementId, IntoElement, RenderOnce, SharedString, Window, div, prelude::*, px};

use crate::theme;

/// A styled text input component.
///
/// This is a display-only component - keyboard input handling remains
/// in the parent view. The component renders:
/// - Themed background and border
/// - Placeholder text when empty (muted color)
/// - Value text when not empty (bright color)
/// - Cursor indicator (`|`) appended to value when focused and non-empty
///
/// # Example
///
/// ```ignore
/// TextInput::new("branch-input")
///     .value(&branch_name)
///     .placeholder("Type branch name...")
///     .focused(is_branch_focused)
/// ```
#[derive(IntoElement)]
pub struct TextInput {
    id: ElementId,
    value: String,
    placeholder: SharedString,
    focused: bool,
}

impl TextInput {
    /// Create a new text input with the given ID.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            value: String::new(),
            placeholder: SharedString::default(),
            focused: false,
        }
    }

    /// Set the current value to display.
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self
    }

    /// Set the placeholder text shown when value is empty.
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Set whether the input is currently focused.
    ///
    /// When focused, the border color changes to Ice. A cursor (`|`) is appended
    /// to the value if non-empty. Empty focused inputs show only the placeholder.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl RenderOnce for TextInput {
    fn render(self, _window: &mut Window, _cx: &mut gpui::App) -> impl IntoElement {
        let is_empty = self.value.is_empty();

        // Determine what text to display
        let display_text = if is_empty {
            self.placeholder.to_string()
        } else if self.focused {
            format!("{}|", self.value)
        } else {
            self.value.clone()
        };

        // Determine text color
        let text_color = if is_empty {
            theme::text_muted()
        } else {
            theme::text_bright()
        };

        // Determine border color based on focus
        let border_color = if self.focused {
            theme::ice()
        } else {
            theme::border()
        };

        div()
            .id(self.id)
            .px(px(theme::SPACE_3))
            .py(px(theme::SPACE_2))
            .bg(theme::surface())
            .rounded(px(theme::RADIUS_MD))
            .border_1()
            .border_color(border_color)
            .min_h(px(36.0))
            .text_color(text_color)
            .child(display_text)
    }
}
