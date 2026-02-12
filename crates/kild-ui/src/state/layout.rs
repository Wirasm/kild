// Allow dead_code — layout state is consumed as split-pane and sidebar modes are wired up.
#![allow(dead_code)]

use std::collections::HashSet;

use crate::views::split_pane::SplitDirection;

/// Sidebar display mode.
#[derive(Debug, Clone, PartialEq, Eq)]

pub enum SidebarMode {
    List,
    Detail { kild_id: String },
}

/// Split pane configuration — stores IDs, not entities.
#[derive(Debug, Clone)]
pub struct SplitConfig {
    direction: SplitDirection,
    /// The terminal ID for the second pane (first pane is always the focused kild).
    second_id: String,
    /// Split ratio (0.1 to 0.9, default 0.5).
    ratio: f32,
}

impl SplitConfig {
    /// Create a new split config with ratio clamped to [0.1, 0.9].
    pub fn new(direction: SplitDirection, second_id: String, ratio: f32) -> Self {
        Self {
            direction,
            second_id,
            ratio: ratio.clamp(0.1, 0.9),
        }
    }

    pub fn direction(&self) -> SplitDirection {
        self.direction
    }

    pub fn second_id(&self) -> &str {
        &self.second_id
    }

    pub fn ratio(&self) -> f32 {
        self.ratio
    }

    /// Update the ratio, clamping to [0.1, 0.9].
    pub(crate) fn set_ratio(&mut self, ratio: f32) {
        self.ratio = ratio.clamp(0.1, 0.9);
    }
}

/// Layout state for the multiplexer view.
pub struct LayoutState {
    /// ID of the kild currently focused in the main area.
    focused_kild: Option<String>,
    /// Set of kild IDs that are minimized (shown as bars at bottom).
    minimized: HashSet<String>,
    /// Sidebar mode: list view or detail view for selected kild.
    sidebar_mode: SidebarMode,
    /// Active split configuration (None = single pane).
    split: Option<SplitConfig>,
    /// Saved layout before maximize (for restore).
    saved_split: Option<SplitConfig>,
}

impl Default for LayoutState {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutState {
    pub fn new() -> Self {
        Self {
            focused_kild: None,
            minimized: HashSet::new(),
            sidebar_mode: SidebarMode::List,
            split: None,
            saved_split: None,
        }
    }

    /// Set the focused kild, removing it from minimized if present.
    pub fn focus_kild(&mut self, id: &str) {
        self.minimized.remove(id);
        self.focused_kild = Some(id.to_string());
    }

    /// Minimize a kild, clearing focus if it was the focused one.
    pub fn minimize_kild(&mut self, id: &str) {
        self.minimized.insert(id.to_string());
        if self.focused_kild.as_deref() == Some(id) {
            self.focused_kild = None;
        }
    }

    /// Toggle sidebar between List and Detail for the given kild.
    pub fn toggle_sidebar_detail(&mut self, kild_id: &str) {
        if self.sidebar_mode
            == (SidebarMode::Detail {
                kild_id: kild_id.to_string(),
            })
        {
            self.sidebar_mode = SidebarMode::List;
        } else {
            self.sidebar_mode = SidebarMode::Detail {
                kild_id: kild_id.to_string(),
            };
        }
    }

    pub fn focused_kild(&self) -> Option<&str> {
        self.focused_kild.as_deref()
    }

    pub fn is_minimized(&self, kild_id: &str) -> bool {
        self.minimized.contains(kild_id)
    }

    pub fn minimized_ids(&self) -> &HashSet<String> {
        &self.minimized
    }

    pub fn sidebar_mode(&self) -> &SidebarMode {
        &self.sidebar_mode
    }

    // =========================================================================
    // Split pane management
    // =========================================================================

    /// Get the current split config.
    pub fn split(&self) -> Option<&SplitConfig> {
        self.split.as_ref()
    }

    /// Split the main area with a second pane.
    pub fn split_with(&mut self, direction: SplitDirection, second_id: String) {
        self.split = Some(SplitConfig::new(direction, second_id, 0.5));
    }

    /// Close the split, returning to single pane.
    pub fn unsplit(&mut self) {
        self.split = None;
    }

    /// Maximize the focused pane (save and clear split).
    pub fn maximize(&mut self) {
        self.saved_split = self.split.take();
    }

    /// Restore from maximized (bring back saved split).
    pub fn restore(&mut self) {
        if let Some(saved) = self.saved_split.take() {
            self.split = Some(saved);
        }
    }

    /// Check if currently in split mode.
    pub fn is_split(&self) -> bool {
        self.split.is_some()
    }

    /// Update the split ratio (for resize handle drag).
    pub fn set_split_ratio(&mut self, ratio: f32) {
        if let Some(ref mut split) = self.split {
            split.set_ratio(ratio);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_defaults() {
        let state = LayoutState::new();
        assert!(state.focused_kild().is_none());
        assert!(state.minimized_ids().is_empty());
        assert_eq!(*state.sidebar_mode(), SidebarMode::List);
        assert!(!state.is_split());
    }

    #[test]
    fn test_focus_and_minimize_cycle() {
        let mut state = LayoutState::new();

        state.focus_kild("k1");
        assert_eq!(state.focused_kild(), Some("k1"));
        assert!(!state.is_minimized("k1"));

        state.minimize_kild("k1");
        assert!(state.focused_kild().is_none());
        assert!(state.is_minimized("k1"));

        // Re-focus removes from minimized
        state.focus_kild("k1");
        assert_eq!(state.focused_kild(), Some("k1"));
        assert!(!state.is_minimized("k1"));
    }

    #[test]
    fn test_focus_removes_from_minimized() {
        let mut state = LayoutState::new();

        state.minimize_kild("k1");
        state.minimize_kild("k2");
        assert!(state.is_minimized("k1"));
        assert!(state.is_minimized("k2"));

        state.focus_kild("k1");
        assert!(!state.is_minimized("k1"));
        assert!(state.is_minimized("k2"));
    }

    #[test]
    fn test_minimize_only_clears_matching_focus() {
        let mut state = LayoutState::new();

        state.focus_kild("k1");
        state.minimize_kild("k2");

        // k1 stays focused because we minimized k2
        assert_eq!(state.focused_kild(), Some("k1"));
        assert!(state.is_minimized("k2"));
    }

    #[test]
    fn test_toggle_sidebar_detail() {
        let mut state = LayoutState::new();
        assert_eq!(*state.sidebar_mode(), SidebarMode::List);

        // Toggle to detail
        state.toggle_sidebar_detail("k1");
        assert_eq!(
            *state.sidebar_mode(),
            SidebarMode::Detail {
                kild_id: "k1".to_string()
            }
        );

        // Toggle same kild back to list
        state.toggle_sidebar_detail("k1");
        assert_eq!(*state.sidebar_mode(), SidebarMode::List);

        // Toggle to detail for k1, then toggle for different kild switches
        state.toggle_sidebar_detail("k1");
        state.toggle_sidebar_detail("k2");
        assert_eq!(
            *state.sidebar_mode(),
            SidebarMode::Detail {
                kild_id: "k2".to_string()
            }
        );
    }

    #[test]
    fn test_split_and_unsplit() {
        let mut state = LayoutState::new();
        assert!(!state.is_split());

        state.split_with(SplitDirection::Vertical, "pane-2".to_string());
        assert!(state.is_split());
        let split = state.split().unwrap();
        assert_eq!(split.direction(), SplitDirection::Vertical);
        assert_eq!(split.second_id(), "pane-2");
        assert!((split.ratio() - 0.5).abs() < f32::EPSILON);

        state.unsplit();
        assert!(!state.is_split());
    }

    #[test]
    fn test_maximize_and_restore() {
        let mut state = LayoutState::new();
        state.split_with(SplitDirection::Horizontal, "pane-2".to_string());

        state.maximize();
        assert!(!state.is_split());

        state.restore();
        assert!(state.is_split());
        assert_eq!(state.split().unwrap().second_id(), "pane-2");
    }

    #[test]
    fn test_set_split_ratio_clamps() {
        let mut state = LayoutState::new();
        state.split_with(SplitDirection::Vertical, "pane-2".to_string());

        state.set_split_ratio(0.0);
        assert!((state.split().unwrap().ratio() - 0.1).abs() < f32::EPSILON);

        state.set_split_ratio(1.0);
        assert!((state.split().unwrap().ratio() - 0.9).abs() < f32::EPSILON);

        state.set_split_ratio(0.3);
        assert!((state.split().unwrap().ratio() - 0.3).abs() < f32::EPSILON);
    }
}
