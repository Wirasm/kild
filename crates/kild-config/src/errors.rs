use std::error::Error;

#[derive(Debug)]
pub enum ConfigError {
    ConfigNotFound {
        path: String,
    },
    ConfigParseError {
        message: String,
    },
    InvalidAgent {
        agent: String,
        supported_agents: String,
    },
    InvalidConfiguration {
        message: String,
    },
    IoError {
        source: std::io::Error,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::ConfigNotFound { path } => {
                write!(f, "Config file not found at '{}'", path)
            }
            ConfigError::ConfigParseError { message } => {
                write!(f, "Failed to parse config file: {}", message)
            }
            ConfigError::InvalidAgent {
                agent,
                supported_agents,
            } => {
                write!(
                    f,
                    "Invalid agent '{}'. Supported agents: {}",
                    agent, supported_agents
                )
            }
            ConfigError::InvalidConfiguration { message } => {
                write!(f, "Invalid configuration: {}", message)
            }
            ConfigError::IoError { source } => {
                write!(f, "IO error reading config: {}", source)
            }
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ConfigError::IoError { source } => Some(source),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ConfigError {
    fn from(source: std::io::Error) -> Self {
        ConfigError::IoError { source }
    }
}
