//! Agent team integration for kild-ui.
//!
//! Feature boundary: all team-related code is isolated here.
//! Removing this module requires only removing `mod teams;`,
//! two MainView fields, and one `render_sidebar` parameter.

mod state;

pub use state::TeamManager;

use crate::theme;
use gpui::Rgba;
use kild_teams::TeamColor;

/// Map Claude Code team colors to the Tallinn Night palette.
pub fn team_color_to_rgba(color: &TeamColor) -> Rgba {
    match color {
        TeamColor::Red => theme::ember(),
        TeamColor::Blue => theme::ice(),
        TeamColor::Green => theme::aurora(),
        TeamColor::Yellow => gpui::rgb(0xE5C07B),
        TeamColor::Purple => gpui::rgb(0xC678DD),
        TeamColor::Orange => theme::copper(),
        TeamColor::Pink => gpui::rgb(0xFF7EB6),
        TeamColor::Cyan => gpui::rgb(0x56B6C2),
        TeamColor::Unknown => theme::text_muted(),
    }
}
