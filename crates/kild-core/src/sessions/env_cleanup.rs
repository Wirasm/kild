//! Environment variable cleanup for spawned agent sessions.
//!
//! Wraps [`kild_protocol::env_cleanup::ENV_VARS_TO_STRIP`] with shell command
//! construction for terminal-mode sessions.

pub use kild_protocol::env_cleanup::ENV_VARS_TO_STRIP;

/// Build a terminal command wrapped with `env` to strip nesting-detection vars
/// and inject additional environment variables.
///
/// Produces: `env -u VAR1 -u VAR2 KEY1=val1 KEY2=val2 <command>`
pub fn build_env_command(env_vars: &[(String, String)], command: &str) -> String {
    let mut parts: Vec<String> = vec!["env".to_string()];

    for var in ENV_VARS_TO_STRIP {
        parts.push(format!("-u {}", var));
    }

    for (k, v) in env_vars {
        parts.push(format!("{}={}", k, v));
    }

    parts.push(command.to_string());
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_vars_to_strip_are_valid_posix_names() {
        for var in ENV_VARS_TO_STRIP {
            assert!(
                !var.is_empty(),
                "ENV_VARS_TO_STRIP must not contain empty strings"
            );
            assert!(
                var.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "ENV_VARS_TO_STRIP entry {:?} contains invalid characters \
                 (must be ASCII alphanumeric or underscore)",
                var
            );
            assert!(
                !var.starts_with(|c: char| c.is_ascii_digit()),
                "ENV_VARS_TO_STRIP entry {:?} must not start with a digit",
                var
            );
        }
    }

    #[test]
    fn test_env_vars_to_strip_contains_claudecode() {
        assert!(
            ENV_VARS_TO_STRIP.contains(&"CLAUDECODE"),
            "ENV_VARS_TO_STRIP should include CLAUDECODE"
        );
    }

    #[test]
    fn test_build_env_command_no_extra_vars() {
        let result = build_env_command(&[], "claude --print");
        assert_eq!(result, "env -u CLAUDECODE claude --print");
    }

    #[test]
    fn test_build_env_command_with_env_vars() {
        let env_vars = vec![
            ("TASK_LIST_ID".to_string(), "kild-my-branch".to_string()),
            ("FOO".to_string(), "bar".to_string()),
        ];
        let result = build_env_command(&env_vars, "claude --print");
        assert_eq!(
            result,
            "env -u CLAUDECODE TASK_LIST_ID=kild-my-branch FOO=bar claude --print"
        );
    }

    #[test]
    fn test_build_env_command_unset_args_come_before_set_args() {
        let env_vars = vec![("KEY".to_string(), "val".to_string())];
        let result = build_env_command(&env_vars, "cmd");

        let unset_pos = result.find("-u ").expect("should have -u");
        let set_pos = result.find("KEY=").expect("should have KEY=");
        assert!(
            unset_pos < set_pos,
            "unset args should come before set args"
        );
    }
}
