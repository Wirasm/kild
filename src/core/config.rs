use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub shards_dir: PathBuf,
    pub log_level: String,
}

impl Default for Config {
    fn default() -> Self {
        let home_dir = dirs::home_dir().expect("Could not find home directory");

        Self {
            shards_dir: home_dir.join(".shards"),
            log_level: std::env::var("SHARDS_LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
        }
    }
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn worktrees_dir(&self) -> PathBuf {
        self.shards_dir.join("worktrees")
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.shards_dir.join("sessions")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::new();
        assert!(config.shards_dir.to_string_lossy().contains(".shards"));
        assert_eq!(config.log_level, "info");
    }

    #[test]
    fn test_config_paths() {
        let config = Config::new();
        assert!(
            config
                .worktrees_dir()
                .to_string_lossy()
                .contains("worktrees")
        );
        assert!(
            config
                .sessions_dir()
                .to_string_lossy()
                .contains("sessions")
        );
    }
}
