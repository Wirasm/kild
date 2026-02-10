//! Agent backend module for managing AI coding assistants.
//!
//! This module provides a centralized system for handling agent-specific logic,
//! including command construction, process detection, and CLI availability checking.
//!
//! # Architecture
//!
//! - [`AgentBackend`] - Trait defining the interface for agent implementations
//! - [`AgentType`] - Enum of all supported agent types
//! - [`AgentError`] - Agent-specific error types
//! - [`backends`] - Individual agent backend implementations
//! - [`registry`] - Global registry for agent lookup
//!
//! # Usage
//!
//! ```rust
//! use kild_core::agents::{is_valid_agent, get_default_command, default_agent_name};
//!
//! // Check if an agent is supported
//! assert!(is_valid_agent("claude"));
//! assert!(!is_valid_agent("unknown"));
//!
//! // Get the default command for an agent
//! assert_eq!(get_default_command("claude"), Some("claude"));
//! assert_eq!(get_default_command("kiro"), Some("kiro-cli chat"));
//!
//! // Get the default agent name
//! assert_eq!(default_agent_name(), "claude");
//! ```

pub mod backends;
pub mod errors;
pub mod registry;
pub mod resume;
pub mod traits;
pub mod types;

// Re-export public API
pub use errors::AgentError;
pub use registry::{
    default_agent_name, default_agent_type, get_agent, get_agent_by_type, get_default_command,
    get_process_patterns, is_agent_available, is_valid_agent, supported_agents_string,
    valid_agent_names,
};
pub use traits::AgentBackend;
pub use types::AgentType;
