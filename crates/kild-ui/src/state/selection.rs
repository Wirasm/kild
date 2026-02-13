/// Encapsulates kild selection state.
///
/// Provides a clean API for selecting/deselecting kilds and checking
/// if a selection is still valid after list updates.
#[derive(Clone, Debug, Default)]
pub struct SelectionState {
    /// ID of the currently selected kild, or None if nothing selected.
    selected_id: Option<String>,
}

impl SelectionState {
    /// Select a kild by ID.
    pub fn select(&mut self, id: String) {
        self.selected_id = Some(id);
    }

    /// Clear the selection.
    pub fn clear(&mut self) {
        self.selected_id = None;
    }

    /// Get the selected kild ID, if any.
    pub fn id(&self) -> Option<&str> {
        self.selected_id.as_deref()
    }

    /// Check if a kild is selected.
    #[allow(dead_code)]
    pub fn has_selection(&self) -> bool {
        self.selected_id.is_some()
    }
}
