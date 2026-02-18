//! Type definitions for the main view.

/// Tracks which region of the UI currently has logical focus.
///
/// Used for keyboard routing — determines where key events are dispatched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusRegion {
    Dashboard,
    Terminal,
}

/// Which view is showing in the main area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActiveView {
    /// Terminal tabs per kild (default).
    Control,
    /// Fleet overview with kild cards.
    Dashboard,
    /// Kild detail drill-down (from dashboard card click).
    Detail,
}
