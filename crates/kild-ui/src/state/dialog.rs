use kild_core::DestroySafety;

/// Dialog state for the application.
///
/// Only one dialog can be open at a time. This enum enforces mutual exclusion
/// at compile-time, preventing impossible states like having both the create
/// and confirm dialogs open simultaneously.
#[derive(Clone, Debug, Default)]
pub enum DialogState {
    /// No dialog is open.
    #[default]
    None,
    /// Create kild dialog is open.
    Create {
        form: CreateFormState,
        error: Option<String>,
    },
    /// Confirm destroy dialog is open.
    Confirm {
        /// Branch being destroyed.
        branch: String,
        /// Safety information for the destroy operation.
        /// None if the safety check failed (proceed without warnings).
        safety_info: Option<DestroySafety>,
        error: Option<String>,
    },
    /// Add project dialog is open.
    AddProject { error: Option<String> },
}

impl DialogState {
    /// Returns true if the create dialog is open.
    pub fn is_create(&self) -> bool {
        matches!(self, DialogState::Create { .. })
    }

    /// Returns true if the confirm dialog is open.
    pub fn is_confirm(&self) -> bool {
        matches!(self, DialogState::Confirm { .. })
    }

    /// Returns true if the add project dialog is open.
    pub fn is_add_project(&self) -> bool {
        matches!(self, DialogState::AddProject { .. })
    }

    /// Open the create dialog with default form state.
    pub fn open_create() -> Self {
        DialogState::Create {
            form: CreateFormState::default(),
            error: None,
        }
    }

    /// Open the confirm dialog for destroying a branch.
    pub fn open_confirm(branch: String, safety_info: Option<DestroySafety>) -> Self {
        DialogState::Confirm {
            branch,
            safety_info,
            error: None,
        }
    }

    /// Open the add project dialog.
    pub fn open_add_project() -> Self {
        DialogState::AddProject { error: None }
    }
}

/// Which field is focused in the create dialog.
///
/// Used for the agent selector focus state (Input fields manage their own focus).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum CreateDialogField {
    #[default]
    BranchName,
    Agent,
}

/// Form state for creating a new kild.
///
/// Text input state is managed by gpui-component's `InputState` entities
/// in MainView. This struct only tracks agent selection and focus state.
#[derive(Clone, Debug)]
pub struct CreateFormState {
    selected_agent_index: usize,
    pub focused_field: CreateDialogField,
}

impl CreateFormState {
    /// Get the current selected agent index.
    pub fn selected_agent_index(&self) -> usize {
        self.selected_agent_index
    }

    /// Set selected agent by index, clamping to valid range.
    pub fn set_selected_agent_index(&mut self, index: usize) {
        let agents = kild_core::agents::valid_agent_names();
        self.selected_agent_index = index.min(agents.len().saturating_sub(1));
    }

    /// Get the currently selected agent name.
    ///
    /// Derives the agent name from the index. The index is always valid
    /// because `set_selected_agent_index` clamps to bounds.
    pub fn selected_agent(&self) -> String {
        let agents = kild_core::agents::valid_agent_names();
        agents[self.selected_agent_index].to_string()
    }
}

impl Default for CreateFormState {
    fn default() -> Self {
        let agents = kild_core::agents::valid_agent_names();
        let default_agent = kild_core::agents::default_agent_name();

        if agents.is_empty() {
            tracing::error!(
                event = "ui.create_form.no_agents_available",
                "Agent list is empty - using hardcoded fallback"
            );
            return Self {
                selected_agent_index: 0,
                focused_field: CreateDialogField::default(),
            };
        }

        let index = agents
            .iter()
            .position(|&a| a == default_agent)
            .unwrap_or_else(|| {
                tracing::warn!(
                    event = "ui.create_form.default_agent_not_found",
                    default = default_agent,
                    selected = agents[0],
                    "Default agent not in list, using first available"
                );
                0
            });

        Self {
            selected_agent_index: index,
            focused_field: CreateDialogField::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dialog_state_mutual_exclusion() {
        // DialogState enum enforces mutual exclusion at compile-time.
        // This test documents the invariant.
        let create = DialogState::open_create();
        assert!(create.is_create());
        assert!(!create.is_confirm());
        assert!(!create.is_add_project());

        let confirm = DialogState::open_confirm("test-branch".to_string(), None);
        assert!(!confirm.is_create());
        assert!(confirm.is_confirm());
        assert!(!confirm.is_add_project());

        let add_project = DialogState::open_add_project();
        assert!(!add_project.is_create());
        assert!(!add_project.is_confirm());
        assert!(add_project.is_add_project());

        let none = DialogState::None;
        assert!(!none.is_create());
        assert!(!none.is_confirm());
        assert!(!none.is_add_project());
    }

    #[test]
    fn test_create_dialog_field_default_is_branch_name() {
        let field = CreateDialogField::default();
        assert_eq!(field, CreateDialogField::BranchName);
    }

    #[test]
    fn test_create_form_state_default_focused_field() {
        let form = CreateFormState::default();
        assert_eq!(form.focused_field, CreateDialogField::BranchName);
    }

    #[test]
    fn test_create_form_state_selected_agent_derives_from_index() {
        let mut form = CreateFormState::default();
        let agents = kild_core::agents::valid_agent_names();

        if agents.len() > 1 {
            // Change index and verify selected_agent() returns the correct agent
            form.set_selected_agent_index(1);
            assert_eq!(form.selected_agent(), agents[1]);
        }
    }

    #[test]
    fn test_create_form_state_set_index_clamps_out_of_bounds() {
        let mut form = CreateFormState::default();
        let agents = kild_core::agents::valid_agent_names();

        // Setting an out-of-bounds index should clamp to last valid index
        form.set_selected_agent_index(999);
        assert_eq!(form.selected_agent_index(), agents.len() - 1);
        assert_eq!(form.selected_agent(), agents[agents.len() - 1]);
    }

    #[test]
    fn test_note_allows_spaces() {
        let mut note = String::new();
        let c = ' ';

        // Note field accepts spaces directly (unlike branch name which converts to hyphen)
        if !c.is_control() {
            note.push(c);
        }

        assert_eq!(note, " ");
    }

    #[test]
    fn test_note_rejects_control_characters() {
        let mut note = String::new();

        // Control characters should be rejected
        for c in ['\n', '\r', '\t', '\x00', '\x1b'] {
            if !c.is_control() {
                note.push(c);
            }
        }

        assert!(
            note.is_empty(),
            "Control characters should not be added to note"
        );
    }

    #[test]
    fn test_note_accepts_unicode() {
        let mut note = String::new();

        // Unicode characters should be accepted
        for c in ['日', '本', '語', '🚀', 'é', 'ñ'] {
            if !c.is_control() {
                note.push(c);
            }
        }

        assert_eq!(note, "日本語🚀éñ");
    }

    #[test]
    fn test_branch_name_validation() {
        let mut branch = String::new();

        // Valid characters for branch names
        for c in ['a', 'Z', '0', '-', '_', '/'] {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '/' {
                branch.push(c);
            }
        }
        assert_eq!(branch, "aZ0-_/");

        // Invalid characters should be rejected
        let mut branch2 = String::new();
        for c in [' ', '@', '#', '$', '%', '!'] {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '/' {
                branch2.push(c);
            }
        }
        assert!(branch2.is_empty(), "Invalid characters should be rejected");
    }

    #[test]
    fn test_note_truncation_at_boundary() {
        let note_25_chars = "1234567890123456789012345";
        let note_26_chars = "12345678901234567890123456";

        // 25 chars should not be truncated
        let truncated_25 = if note_25_chars.chars().count() > 25 {
            format!("{}...", note_25_chars.chars().take(25).collect::<String>())
        } else {
            note_25_chars.to_string()
        };
        assert_eq!(truncated_25, note_25_chars);

        // 26 chars should be truncated to "25chars..."
        let truncated_26 = if note_26_chars.chars().count() > 25 {
            format!("{}...", note_26_chars.chars().take(25).collect::<String>())
        } else {
            note_26_chars.to_string()
        };
        assert_eq!(truncated_26, "1234567890123456789012345...");
    }

    #[test]
    fn test_note_truncation_unicode() {
        // Unicode characters should be counted as single characters, not bytes
        let unicode_note = "日本語テスト文字列はここにあります長い"; // 18 chars

        let truncated = if unicode_note.chars().count() > 25 {
            format!("{}...", unicode_note.chars().take(25).collect::<String>())
        } else {
            unicode_note.to_string()
        };

        // Should not be truncated (only 18 chars)
        assert_eq!(truncated, unicode_note);
    }

    #[test]
    fn test_note_trimming_whitespace_only() {
        let note_whitespace = "   \t  \n  ";

        // Whitespace-only note should become None
        let trimmed = if note_whitespace.trim().is_empty() {
            None
        } else {
            Some(note_whitespace.trim().to_string())
        };

        assert!(trimmed.is_none(), "Whitespace-only note should become None");
    }

    #[test]
    fn test_note_trimming_preserves_content() {
        let note_with_spaces = "  hello world  ";

        let trimmed = if note_with_spaces.trim().is_empty() {
            None
        } else {
            Some(note_with_spaces.trim().to_string())
        };

        assert_eq!(trimmed, Some("hello world".to_string()));
    }
}
