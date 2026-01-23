//! shards-ui: GUI for Shards
//!
//! GPUI-based visual dashboard for shard management.
//! See .claude/PRPs/prds/gpui-native-terminal-ui.prd.md for implementation plan.

// Import gpui to verify dependency compiles
use gpui as _;

const PRIMARY_MESSAGE: &str = "shards-ui: GPUI scaffolding ready.";
const SECONDARY_MESSAGE: &str = "See Phase 2 of gpui-native-terminal-ui.prd.md to continue.";

fn main() {
    eprintln!("{PRIMARY_MESSAGE}");
    eprintln!("{SECONDARY_MESSAGE}");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::{PRIMARY_MESSAGE, SECONDARY_MESSAGE};

    #[test]
    fn status_messages_are_current() {
        assert_eq!(PRIMARY_MESSAGE, "shards-ui: GPUI scaffolding ready.");
        assert_eq!(
            SECONDARY_MESSAGE,
            "See Phase 2 of gpui-native-terminal-ui.prd.md to continue."
        );
    }
}
