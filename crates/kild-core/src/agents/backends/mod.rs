//! Agent backend implementations.
//!
//! All backends are defined via the `define_agent_backend!` macro, which generates
//! the struct, `AgentBackend` trait impl, and uniquely named tests. Each invocation
//! requires a `test_prefix` identifier used to produce descriptive test function
//! names via `paste`.

/// Shared test body for both macro arms. Generates the four tests common to all
/// backends; the yolo-specific test is added by each arm individually.
#[cfg(test)]
macro_rules! define_agent_backend_tests {
    ($struct_name:ident, $prefix:ident, $name:expr, $display:expr, $cmd:expr, [$($pat:expr),+]) => {
        paste::paste! {
            #[test]
            fn [<$prefix _backend_returns_correct_name>]() {
                assert_eq!($struct_name.name(), $name);
            }

            #[test]
            fn [<$prefix _backend_returns_correct_display_name>]() {
                assert_eq!($struct_name.display_name(), $display);
            }

            #[test]
            fn [<$prefix _backend_returns_correct_default_command>]() {
                assert_eq!($struct_name.default_command(), $cmd);
            }

            #[test]
            fn [<$prefix _backend_returns_expected_process_patterns>]() {
                let patterns = $struct_name.process_patterns();
                $(assert!(patterns.contains(&$pat.to_string()));)+
            }
        }
    };
}

macro_rules! define_agent_backend {
    // Arm with yolo_flags (agents that support autonomous mode)
    (
        $struct_name:ident,
        test_prefix: $prefix:ident,
        name: $name:expr,
        display_name: $display:expr,
        binary: $binary:expr,
        command: $cmd:expr,
        process_patterns: [$($pat:expr),+ $(,)?],
        yolo_flags: $yolo:expr
    ) => {
        pub struct $struct_name;

        impl crate::agents::traits::AgentBackend for $struct_name {
            fn name(&self) -> &'static str {
                $name
            }

            fn display_name(&self) -> &'static str {
                $display
            }

            fn is_available(&self) -> bool {
                which::which($binary).is_ok()
            }

            fn default_command(&self) -> &'static str {
                $cmd
            }

            fn process_patterns(&self) -> Vec<String> {
                vec![$($pat.to_string()),+]
            }

            fn yolo_flags(&self) -> Option<&'static str> {
                Some($yolo)
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use crate::agents::traits::AgentBackend;

            define_agent_backend_tests!($struct_name, $prefix, $name, $display, $cmd, [$($pat),+]);

            paste::paste! {
                #[test]
                fn [<$prefix _backend_returns_correct_yolo_flags>]() {
                    assert_eq!($struct_name.yolo_flags(), Some($yolo));
                }
            }
        }
    };
    // Arm without yolo_flags (agents that don't support autonomous mode)
    (
        $struct_name:ident,
        test_prefix: $prefix:ident,
        name: $name:expr,
        display_name: $display:expr,
        binary: $binary:expr,
        command: $cmd:expr,
        process_patterns: [$($pat:expr),+ $(,)?]
    ) => {
        pub struct $struct_name;

        impl crate::agents::traits::AgentBackend for $struct_name {
            fn name(&self) -> &'static str {
                $name
            }

            fn display_name(&self) -> &'static str {
                $display
            }

            fn is_available(&self) -> bool {
                which::which($binary).is_ok()
            }

            fn default_command(&self) -> &'static str {
                $cmd
            }

            fn process_patterns(&self) -> Vec<String> {
                vec![$($pat.to_string()),+]
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use crate::agents::traits::AgentBackend;

            define_agent_backend_tests!($struct_name, $prefix, $name, $display, $cmd, [$($pat),+]);

            paste::paste! {
                #[test]
                fn [<$prefix _backend_returns_no_yolo_flags>]() {
                    assert_eq!($struct_name.yolo_flags(), None);
                }
            }
        }
    };
}

mod amp {
    define_agent_backend!(AmpBackend,
        test_prefix: amp,
        name: "amp",
        display_name: "Amp",
        binary: "amp",
        command: "amp",
        process_patterns: ["amp"],
        yolo_flags: "--dangerously-allow-all"
    );
}

mod claude {
    define_agent_backend!(ClaudeBackend,
        test_prefix: claude,
        name: "claude",
        display_name: "Claude Code",
        binary: "claude",
        command: "claude",
        process_patterns: ["claude", "claude-code"],
        yolo_flags: "--dangerously-skip-permissions"
    );
}

mod codex {
    define_agent_backend!(CodexBackend,
        test_prefix: codex,
        name: "codex",
        display_name: "Codex CLI",
        binary: "codex",
        command: "codex",
        process_patterns: ["codex"],
        yolo_flags: "--yolo"
    );
}

mod gemini {
    define_agent_backend!(GeminiBackend,
        test_prefix: gemini,
        name: "gemini",
        display_name: "Gemini CLI",
        binary: "gemini",
        command: "gemini",
        process_patterns: ["gemini", "gemini-cli"],
        yolo_flags: "--yolo --approval-mode yolo"
    );
}

mod kiro {
    define_agent_backend!(KiroBackend,
        test_prefix: kiro,
        name: "kiro",
        display_name: "Kiro CLI",
        binary: "kiro-cli",
        command: "kiro-cli chat",
        process_patterns: ["kiro-cli", "kiro"],
        yolo_flags: "--trust-all-tools"
    );
}

mod opencode {
    define_agent_backend!(OpenCodeBackend,
        test_prefix: opencode,
        name: "opencode",
        display_name: "OpenCode",
        binary: "opencode",
        command: "opencode",
        process_patterns: ["opencode"]
    );
}

pub use amp::AmpBackend;
pub use claude::ClaudeBackend;
pub use codex::CodexBackend;
pub use gemini::GeminiBackend;
pub use kiro::KiroBackend;
pub use opencode::OpenCodeBackend;
