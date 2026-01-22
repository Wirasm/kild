# Prompt Piping Feature - Implementation Plan

> **Status**: Planning
> **Target**: Claude Code first, extensible to other CLIs
> **Author**: Generated via Claude Code session
> **Date**: 2025-01-17

---

## Table of Contents

1. [Overview](#overview)
2. [Goals and Non-Goals](#goals-and-non-goals)
3. [Architecture Design](#architecture-design)
4. [Claude Code Implementation](#claude-code-implementation)
5. [CLI Interface Changes](#cli-interface-changes)
6. [Session Metadata Changes](#session-metadata-changes)
7. [Error Handling](#error-handling)
8. [Test Cases](#test-cases)
9. [Edge Cases](#edge-cases)
10. [Future Work: Other CLIs](#future-work-other-clis)
11. [Implementation Phases](#implementation-phases)
12. [Open Questions](#open-questions)

---

## Overview

### Problem Statement

Currently, Shards creates isolated worktrees and spawns AI agent sessions in terminal windows using a "fire-and-forget" model. Users cannot:

1. Send an initial prompt when creating a shard
2. Track when the initial task completes
3. Programmatically interact with the agent before handing off to interactive mode

### Proposed Solution

Implement a **"Pipe → Track → Open → Resume"** workflow:

```
shards create mybranch --agent claude --prompt "Fix the auth bug"
       │
       ▼
┌──────────────────────────────────────────────────────────────┐
│ 1. Create worktree at .shards/mybranch                       │
│ 2. Run headless: claude -p "Fix auth bug" --output-format json│
│ 3. Wait for completion (command exits when done)             │
│ 4. Capture session_id from JSON output                       │
│ 5. Store session_id in session metadata                      │
│ 6. Open terminal window in worktree                          │
│ 7. Run: claude --resume <session_id>                         │
│ 8. User has interactive session with full context            │
└──────────────────────────────────────────────────────────────┘
```

### Key Insight

Claude Code's `-p` (print/headless) mode:
- Runs a prompt and exits when complete
- Returns JSON with `session_id`
- Session can be resumed with `--resume <session_id>`
- Sessions work across git worktrees in the same repo

This enables clean completion tracking and session handoff.

---

## Goals and Non-Goals

### Goals

- [ ] Allow users to specify an initial prompt when creating a shard
- [ ] Run the prompt in headless mode before opening the terminal
- [ ] Track completion via process exit
- [ ] Preserve session context for interactive continuation
- [ ] Support multiple prompt sources (CLI arg, stdin, file)
- [ ] Design extensible architecture for future CLI support
- [ ] Maintain backward compatibility (no prompt = current behavior)

### Non-Goals (This Phase)

- Implementing support for Codex, Gemini, Aider, Kiro (future phases)
- Real-time streaming of headless output to user
- Sending additional prompts to already-running interactive sessions
- Slash command support in headless mode (Claude Code limitation)
- Daemon/server mode for persistent agent connections

---

## Architecture Design

### Agent Trait System (Extensibility Foundation)

Even though we're implementing Claude Code first, we'll design the trait system upfront to ensure clean architecture.

```rust
// crates/shards-core/src/agents/traits.rs

use std::path::Path;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Result from running an agent in headless mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadlessResult {
    /// Whether the command succeeded
    pub success: bool,
    /// Exit code from the process
    pub exit_code: i32,
    /// Session ID for resuming (if supported)
    pub session_id: Option<String>,
    /// Raw stdout output
    pub stdout: String,
    /// Raw stderr output
    pub stderr: String,
    /// Parsed result/response (agent-specific)
    pub result: Option<String>,
}

/// Agent capabilities for feature detection
#[derive(Debug, Clone, Default)]
pub struct AgentCapabilities {
    /// Can run prompts without interactive terminal
    pub supports_headless: bool,
    /// Can resume previous sessions by ID
    pub supports_resume: bool,
    /// Can output structured JSON
    pub supports_json_output: bool,
    /// Can accept input via stdin pipe
    pub supports_stdin_pipe: bool,
    /// Provides session ID in output
    pub provides_session_id: bool,
}

/// Options for headless execution
#[derive(Debug, Clone, Default)]
pub struct HeadlessOptions {
    /// Timeout in seconds (None = no timeout)
    pub timeout_secs: Option<u64>,
    /// Additional CLI flags to pass
    pub extra_flags: Vec<String>,
    /// Allowed tools (for Claude: --allowedTools)
    pub allowed_tools: Option<String>,
}

/// Core trait for AI coding CLI agents
#[async_trait]
pub trait AgentRunner: Send + Sync {
    /// Unique identifier for this agent (e.g., "claude", "codex")
    fn name(&self) -> &'static str;

    /// Human-readable display name
    fn display_name(&self) -> &'static str;

    /// Query agent capabilities
    fn capabilities(&self) -> AgentCapabilities;

    /// Check if the agent binary is installed and accessible
    fn is_available(&self) -> bool;

    /// Get the binary path/name
    fn binary(&self) -> &str;

    /// Run a prompt in headless mode
    async fn run_headless(
        &self,
        prompt: &str,
        working_dir: &Path,
        options: &HeadlessOptions,
    ) -> Result<HeadlessResult, AgentError>;

    /// Build command string to resume a session interactively
    fn build_resume_command(&self, session_id: &str) -> Option<String>;

    /// Build command string to start fresh interactive session
    fn build_interactive_command(&self) -> String;

    /// Parse session ID from headless output (stdout)
    fn parse_session_id(&self, stdout: &str) -> Option<String>;
}

/// Agent-specific errors
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("Agent binary not found: {binary}")]
    BinaryNotFound { binary: String },

    #[error("Headless execution failed: {message}")]
    HeadlessFailed { message: String, exit_code: i32 },

    #[error("Timeout after {seconds} seconds")]
    Timeout { seconds: u64 },

    #[error("Failed to parse output: {message}")]
    ParseError { message: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Session resume not supported by this agent")]
    ResumeNotSupported,

    #[error("Headless mode not supported by this agent")]
    HeadlessNotSupported,
}
```

### Agent Registry

```rust
// crates/shards-core/src/agents/registry.rs

use std::collections::HashMap;
use super::traits::AgentRunner;

pub struct AgentRegistry {
    agents: HashMap<String, Box<dyn AgentRunner>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self { agents: HashMap::new() }
    }

    /// Create registry with all built-in agents
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();

        // Phase 1: Claude only
        registry.register(Box::new(super::claude::ClaudeAgent::default()));

        // Future phases will add:
        // registry.register(Box::new(super::codex::CodexAgent::default()));
        // registry.register(Box::new(super::gemini::GeminiAgent::default()));
        // registry.register(Box::new(super::aider::AiderAgent::default()));
        // registry.register(Box::new(super::kiro::KiroAgent::default()));

        registry
    }

    pub fn register(&mut self, agent: Box<dyn AgentRunner>) {
        self.agents.insert(agent.name().to_string(), agent);
    }

    pub fn get(&self, name: &str) -> Option<&dyn AgentRunner> {
        self.agents.get(name).map(|a| a.as_ref())
    }

    pub fn list(&self) -> Vec<&dyn AgentRunner> {
        self.agents.values().map(|a| a.as_ref()).collect()
    }

    pub fn list_available(&self) -> Vec<&dyn AgentRunner> {
        self.agents
            .values()
            .filter(|a| a.is_available())
            .map(|a| a.as_ref())
            .collect()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}
```

---

## Claude Code Implementation

### ClaudeAgent Structure

```rust
// crates/shards-core/src/agents/claude.rs

use std::path::Path;
use std::process::Stdio;
use async_trait::async_trait;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use serde_json::Value;

use super::traits::{
    AgentRunner, AgentCapabilities, AgentError,
    HeadlessOptions, HeadlessResult
};

#[derive(Debug, Clone)]
pub struct ClaudeAgent {
    /// Binary name or path (default: "claude")
    pub binary: String,
    /// Default flags to pass to all invocations
    pub default_flags: Vec<String>,
}

impl Default for ClaudeAgent {
    fn default() -> Self {
        Self {
            binary: "claude".to_string(),
            default_flags: vec![],
        }
    }
}

impl ClaudeAgent {
    pub fn new(binary: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
            default_flags: vec![],
        }
    }

    pub fn with_flags(mut self, flags: Vec<String>) -> Self {
        self.default_flags = flags;
        self
    }
}

#[async_trait]
impl AgentRunner for ClaudeAgent {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn display_name(&self) -> &'static str {
        "Claude Code"
    }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            supports_headless: true,
            supports_resume: true,
            supports_json_output: true,
            supports_stdin_pipe: true,
            provides_session_id: true,
        }
    }

    fn is_available(&self) -> bool {
        which::which(&self.binary).is_ok()
    }

    fn binary(&self) -> &str {
        &self.binary
    }

    async fn run_headless(
        &self,
        prompt: &str,
        working_dir: &Path,
        options: &HeadlessOptions,
    ) -> Result<HeadlessResult, AgentError> {
        // Build command
        let mut cmd = Command::new(&self.binary);
        cmd.current_dir(working_dir)
           .arg("-p")
           .arg(prompt)
           .arg("--output-format")
           .arg("json")
           .stdout(Stdio::piped())
           .stderr(Stdio::piped());

        // Add default flags
        for flag in &self.default_flags {
            cmd.arg(flag);
        }

        // Add allowed tools if specified
        if let Some(tools) = &options.allowed_tools {
            cmd.arg("--allowedTools").arg(tools);
        }

        // Add extra flags
        for flag in &options.extra_flags {
            cmd.arg(flag);
        }

        // Execute with optional timeout
        let output = if let Some(secs) = options.timeout_secs {
            timeout(Duration::from_secs(secs), cmd.output())
                .await
                .map_err(|_| AgentError::Timeout { seconds: secs })??
        } else {
            cmd.output().await?
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        // Parse JSON output
        let (session_id, result) = if output.status.success() {
            match serde_json::from_str::<Value>(&stdout) {
                Ok(json) => {
                    let sid = json.get("session_id")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let res = json.get("result")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    (sid, res)
                }
                Err(_) => (None, None),
            }
        } else {
            (None, None)
        };

        Ok(HeadlessResult {
            success: output.status.success(),
            exit_code,
            session_id,
            stdout,
            stderr,
            result,
        })
    }

    fn build_resume_command(&self, session_id: &str) -> Option<String> {
        let flags = if self.default_flags.is_empty() {
            String::new()
        } else {
            format!(" {}", self.default_flags.join(" "))
        };
        Some(format!("claude --resume {}{}", session_id, flags))
    }

    fn build_interactive_command(&self) -> String {
        if self.default_flags.is_empty() {
            "claude".to_string()
        } else {
            format!("claude {}", self.default_flags.join(" "))
        }
    }

    fn parse_session_id(&self, stdout: &str) -> Option<String> {
        // Try to parse as JSON first
        if let Ok(json) = serde_json::from_str::<Value>(stdout) {
            if let Some(sid) = json.get("session_id").and_then(|v| v.as_str()) {
                return Some(sid.to_string());
            }
        }

        // Fallback: try to find session_id in output (for streaming JSON)
        for line in stdout.lines() {
            if let Ok(json) = serde_json::from_str::<Value>(line) {
                if let Some(sid) = json.get("session_id").and_then(|v| v.as_str()) {
                    return Some(sid.to_string());
                }
            }
        }

        None
    }
}
```

### Integration with Session Handler

```rust
// crates/shards-core/src/sessions/handler.rs (modifications)

use crate::agents::{AgentRegistry, traits::{HeadlessOptions, AgentError}};

/// Options for creating a session with optional prompt
#[derive(Debug, Clone, Default)]
pub struct CreateSessionOptions {
    /// Initial prompt to run in headless mode before opening terminal
    pub prompt: Option<String>,
    /// Read prompt from this file path
    pub prompt_file: Option<PathBuf>,
    /// Read prompt from stdin (when value is "-")
    pub prompt_stdin: bool,
    /// Timeout for headless execution in seconds
    pub timeout_secs: Option<u64>,
    /// Skip opening terminal after headless (keep session for later)
    pub no_terminal: bool,
    /// Additional flags for the agent
    pub agent_flags: Vec<String>,
    /// Allowed tools for headless execution
    pub allowed_tools: Option<String>,
}

/// Result of session creation
#[derive(Debug)]
pub struct CreateSessionResult {
    pub session: Session,
    /// Output from headless execution (if prompt was provided)
    pub headless_output: Option<String>,
    /// Whether headless execution succeeded
    pub headless_success: Option<bool>,
}

pub async fn create_session(
    branch: &str,
    agent_name: &str,
    options: CreateSessionOptions,
    config: &ShardsConfig,
    registry: &AgentRegistry,
) -> Result<CreateSessionResult, SessionError> {
    // 1. Validate agent exists and is available
    let agent = registry.get(agent_name)
        .ok_or_else(|| SessionError::UnknownAgent {
            agent: agent_name.to_string(),
            available: registry.list().iter().map(|a| a.name().to_string()).collect(),
        })?;

    if !agent.is_available() {
        return Err(SessionError::AgentNotInstalled {
            agent: agent_name.to_string(),
            binary: agent.binary().to_string(),
        });
    }

    // 2. Resolve prompt from various sources
    let prompt = resolve_prompt(&options).await?;

    // 3. Create worktree (existing logic)
    let worktree_path = create_worktree(branch, config)?;

    // 4. Run headless if prompt provided
    let (agent_session_id, headless_output, headless_success) =
        if let Some(prompt_text) = &prompt {
            // Check agent supports headless
            if !agent.capabilities().supports_headless {
                return Err(SessionError::HeadlessNotSupported {
                    agent: agent_name.to_string(),
                });
            }

            let headless_opts = HeadlessOptions {
                timeout_secs: options.timeout_secs,
                extra_flags: options.agent_flags.clone(),
                allowed_tools: options.allowed_tools.clone(),
            };

            match agent.run_headless(prompt_text, &worktree_path, &headless_opts).await {
                Ok(result) => {
                    if !result.success {
                        // Log warning but continue - user may want to debug in terminal
                        tracing::warn!(
                            "Headless execution failed with exit code {}: {}",
                            result.exit_code,
                            result.stderr
                        );
                    }
                    (result.session_id, Some(result.stdout), Some(result.success))
                }
                Err(AgentError::Timeout { seconds }) => {
                    return Err(SessionError::HeadlessTimeout { seconds });
                }
                Err(e) => {
                    return Err(SessionError::HeadlessFailed {
                        message: e.to_string()
                    });
                }
            }
        } else {
            (None, None, None)
        };

    // 5. Determine terminal command
    let terminal_command = if let Some(ref sid) = agent_session_id {
        // Resume the headless session
        agent.build_resume_command(sid)
            .unwrap_or_else(|| agent.build_interactive_command())
    } else {
        // Fresh interactive session
        agent.build_interactive_command()
    };

    // 6. Spawn terminal (unless --no-terminal)
    let process_info = if !options.no_terminal {
        Some(spawn_terminal(&worktree_path, &terminal_command, config)?)
    } else {
        None
    };

    // 7. Create session record
    let session = Session {
        name: branch.to_string(),
        branch: format!("shard_{}", generate_hash(branch)),
        worktree_path: worktree_path.clone(),
        agent: agent_name.to_string(),
        agent_session_id,  // NEW FIELD
        initial_prompt: prompt,  // NEW FIELD
        status: if options.no_terminal {
            SessionStatus::Headless
        } else {
            SessionStatus::Interactive
        },
        process_id: process_info.as_ref().map(|p| p.process_id),
        process_name: process_info.as_ref().and_then(|p| p.process_name.clone()),
        process_start_time: process_info.as_ref().and_then(|p| p.process_start_time),
        created_at: chrono::Utc::now(),
        command: terminal_command,  // Store for reference
    };

    // 8. Save session
    save_session(&session)?;

    Ok(CreateSessionResult {
        session,
        headless_output,
        headless_success,
    })
}

/// Resolve prompt from CLI arg, file, or stdin
async fn resolve_prompt(options: &CreateSessionOptions) -> Result<Option<String>, SessionError> {
    // Priority: stdin > file > direct arg
    if options.prompt_stdin {
        let mut buffer = String::new();
        tokio::io::AsyncReadExt::read_to_string(
            &mut tokio::io::stdin(),
            &mut buffer
        ).await?;

        let trimmed = buffer.trim();
        if trimmed.is_empty() {
            return Err(SessionError::EmptyPrompt { source: "stdin".to_string() });
        }
        return Ok(Some(trimmed.to_string()));
    }

    if let Some(path) = &options.prompt_file {
        let content = tokio::fs::read_to_string(path).await
            .map_err(|e| SessionError::PromptFileError {
                path: path.clone(),
                error: e.to_string()
            })?;

        let trimmed = content.trim();
        if trimmed.is_empty() {
            return Err(SessionError::EmptyPrompt {
                source: path.display().to_string()
            });
        }
        return Ok(Some(trimmed.to_string()));
    }

    Ok(options.prompt.clone())
}
```

---

## CLI Interface Changes

### Updated Create Command

```rust
// crates/shards/src/app.rs

#[derive(Parser)]
pub struct CreateCommand {
    /// Branch name for the shard
    #[arg(value_name = "BRANCH")]
    pub branch: String,

    /// Agent to use (claude, codex, gemini, aider, kiro)
    #[arg(short, long, default_value = "claude")]
    pub agent: String,

    /// Initial prompt to run in headless mode before opening terminal
    #[arg(short, long, value_name = "PROMPT")]
    pub prompt: Option<String>,

    /// Read prompt from file
    #[arg(long, value_name = "FILE")]
    pub prompt_file: Option<PathBuf>,

    /// Read prompt from stdin (use "-" as value)
    /// Example: echo "Fix the bug" | shards create mybranch --prompt -
    #[arg(long = "prompt-stdin", visible_alias = "prompt=-")]
    pub prompt_stdin: bool,

    /// Timeout for headless execution in seconds
    #[arg(long, value_name = "SECONDS")]
    pub timeout: Option<u64>,

    /// Don't open terminal after headless execution
    #[arg(long)]
    pub no_terminal: bool,

    /// Allowed tools for headless execution (Claude: --allowedTools)
    #[arg(long, value_name = "TOOLS")]
    pub allowed_tools: Option<String>,

    /// Additional flags to pass to the agent
    #[arg(long = "flag", short = 'f', value_name = "FLAG")]
    pub flags: Vec<String>,
}
```

### New Commands

```rust
// crates/shards/src/app.rs

#[derive(Subcommand)]
pub enum Commands {
    // Existing
    Create(CreateCommand),
    List(ListCommand),
    Destroy(DestroyCommand),
    Status(StatusCommand),
    Cleanup(CleanupCommand),

    // New commands
    /// Send a prompt to a shard (runs headless, updates session)
    Send(SendCommand),

    /// Open/attach to a shard's terminal
    Attach(AttachCommand),

    /// List available agents and their capabilities
    Agents(AgentsCommand),
}

#[derive(Parser)]
pub struct SendCommand {
    /// Shard name
    #[arg(value_name = "NAME")]
    pub name: String,

    /// Prompt to send
    #[arg(value_name = "PROMPT")]
    pub prompt: Option<String>,

    /// Read prompt from file
    #[arg(long, value_name = "FILE")]
    pub prompt_file: Option<PathBuf>,

    /// Read prompt from stdin
    #[arg(long)]
    pub prompt_stdin: bool,

    /// Open terminal after completion
    #[arg(long)]
    pub then_open: bool,

    /// Timeout in seconds
    #[arg(long, value_name = "SECONDS")]
    pub timeout: Option<u64>,
}

#[derive(Parser)]
pub struct AttachCommand {
    /// Shard name
    #[arg(value_name = "NAME")]
    pub name: String,
}

#[derive(Parser)]
pub struct AgentsCommand {
    /// Show detailed capabilities
    #[arg(long)]
    pub verbose: bool,
}
```

### Usage Examples

```bash
# Basic: create with prompt
shards create auth-fix --agent claude --prompt "Fix the authentication bug in login.rs"

# From file
shards create feature-x --agent claude --prompt-file ./tasks/feature-x.md

# From stdin (pipe)
echo "Add unit tests for the User model" | shards create add-tests --prompt -

# With timeout
shards create big-refactor --prompt "Refactor the entire auth module" --timeout 300

# Headless only (no terminal)
shards create batch-job --prompt "Update all copyright headers" --no-terminal

# With allowed tools
shards create fix-tests --prompt "Fix failing tests" --allowed-tools "Bash(npm:*),Read,Edit"

# Send follow-up prompt
shards send auth-fix "Now add tests for the fix"

# Attach to session
shards attach auth-fix

# List agents
shards agents
shards agents --verbose
```

---

## Session Metadata Changes

### Updated Session Struct

```rust
// crates/shards-core/src/sessions/types.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Shard name (user-provided)
    pub name: String,

    /// Git branch name (generated: shard_<hash>)
    pub branch: String,

    /// Path to worktree
    pub worktree_path: PathBuf,

    /// Agent used (claude, codex, etc.)
    pub agent: String,

    // --- NEW FIELDS ---

    /// Agent's session ID (for resume support)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,

    /// Initial prompt that was run (if any)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_prompt: Option<String>,

    /// History of prompts sent to this shard
    #[serde(default)]
    pub prompt_history: Vec<PromptRecord>,

    /// Current session status
    pub status: SessionStatus,

    /// Command used to start/resume the session
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    // --- EXISTING FIELDS ---

    /// Process ID (of terminal or agent)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<u32>,

    /// Process name for validation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_name: Option<String>,

    /// Process start time for PID reuse protection
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_start_time: Option<u64>,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Port range start
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_range_start: Option<u16>,

    /// Number of ports allocated
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_count: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptRecord {
    /// The prompt text
    pub prompt: String,
    /// When it was sent
    pub sent_at: DateTime<Utc>,
    /// Whether it completed successfully
    pub success: bool,
    /// Session ID after this prompt (may change)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionStatus {
    /// Running in headless mode (no terminal)
    Headless,
    /// Running in interactive terminal
    Interactive,
    /// Terminal closed, can be resumed
    Suspended,
    /// Explicitly stopped
    Stopped,
}
```

### Example Session JSON

```json
{
  "name": "auth-fix",
  "branch": "shard_a1b2c3d4",
  "worktree_path": "/repo/.shards/auth-fix",
  "agent": "claude",
  "agent_session_id": "550e8400-e29b-41d4-a716-446655440000",
  "initial_prompt": "Fix the authentication bug in login.rs",
  "prompt_history": [
    {
      "prompt": "Fix the authentication bug in login.rs",
      "sent_at": "2025-01-17T10:30:00Z",
      "success": true,
      "session_id": "550e8400-e29b-41d4-a716-446655440000"
    },
    {
      "prompt": "Now add tests for the fix",
      "sent_at": "2025-01-17T11:00:00Z",
      "success": true,
      "session_id": "550e8400-e29b-41d4-a716-446655440000"
    }
  ],
  "status": "Interactive",
  "command": "claude --resume 550e8400-e29b-41d4-a716-446655440000",
  "process_id": 12345,
  "process_name": "claude",
  "process_start_time": 1705487400,
  "created_at": "2025-01-17T10:30:00Z",
  "port_range_start": 8100,
  "port_count": 10
}
```

---

## Error Handling

### Error Types

```rust
// crates/shards-core/src/sessions/errors.rs (additions)

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    // Existing errors...

    // --- NEW ERRORS ---

    #[error("Unknown agent '{agent}'. Available: {}", available.join(", "))]
    UnknownAgent {
        agent: String,
        available: Vec<String>,
    },

    #[error("Agent '{agent}' is not installed. Binary '{binary}' not found in PATH.")]
    AgentNotInstalled {
        agent: String,
        binary: String,
    },

    #[error("Agent '{agent}' does not support headless mode")]
    HeadlessNotSupported {
        agent: String,
    },

    #[error("Headless execution timed out after {seconds} seconds")]
    HeadlessTimeout {
        seconds: u64,
    },

    #[error("Headless execution failed: {message}")]
    HeadlessFailed {
        message: String,
    },

    #[error("Empty prompt from {source}")]
    EmptyPrompt {
        source: String,
    },

    #[error("Failed to read prompt file '{path}': {error}")]
    PromptFileError {
        path: PathBuf,
        error: String,
    },

    #[error("Session '{name}' does not have a resumable session ID")]
    NoSessionId {
        name: String,
    },

    #[error("Agent '{agent}' does not support session resume")]
    ResumeNotSupported {
        agent: String,
    },
}
```

### Error Recovery Strategies

| Error | Recovery Strategy |
|-------|-------------------|
| Headless timeout | Cancel, report timeout, don't open terminal |
| Headless fails (non-zero exit) | Log warning, open terminal anyway (user can debug) |
| No session ID returned | Open fresh interactive session instead |
| Agent not installed | Fail early with helpful message |
| Empty prompt | Fail with clear error |
| Prompt file not found | Fail with path in error message |

---

## Test Cases

### Unit Tests

```rust
// crates/shards-core/src/agents/claude_test.rs

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // --- Capability Tests ---

    #[test]
    fn test_claude_capabilities() {
        let agent = ClaudeAgent::default();
        let caps = agent.capabilities();

        assert!(caps.supports_headless);
        assert!(caps.supports_resume);
        assert!(caps.supports_json_output);
        assert!(caps.supports_stdin_pipe);
        assert!(caps.provides_session_id);
    }

    #[test]
    fn test_claude_name() {
        let agent = ClaudeAgent::default();
        assert_eq!(agent.name(), "claude");
        assert_eq!(agent.display_name(), "Claude Code");
    }

    // --- Command Building Tests ---

    #[test]
    fn test_build_interactive_command() {
        let agent = ClaudeAgent::default();
        assert_eq!(agent.build_interactive_command(), "claude");

        let agent_with_flags = ClaudeAgent::default()
            .with_flags(vec!["--verbose".to_string()]);
        assert_eq!(agent_with_flags.build_interactive_command(), "claude --verbose");
    }

    #[test]
    fn test_build_resume_command() {
        let agent = ClaudeAgent::default();
        let cmd = agent.build_resume_command("abc-123");
        assert_eq!(cmd, Some("claude --resume abc-123".to_string()));
    }

    #[test]
    fn test_build_resume_command_with_flags() {
        let agent = ClaudeAgent::default()
            .with_flags(vec!["--verbose".to_string()]);
        let cmd = agent.build_resume_command("abc-123");
        assert_eq!(cmd, Some("claude --resume abc-123 --verbose".to_string()));
    }

    // --- Session ID Parsing Tests ---

    #[test]
    fn test_parse_session_id_valid_json() {
        let agent = ClaudeAgent::default();
        let output = r#"{"session_id": "550e8400-e29b-41d4-a716-446655440000", "result": "done"}"#;

        let sid = agent.parse_session_id(output);
        assert_eq!(sid, Some("550e8400-e29b-41d4-a716-446655440000".to_string()));
    }

    #[test]
    fn test_parse_session_id_streaming_json() {
        let agent = ClaudeAgent::default();
        let output = r#"{"type": "start"}
{"type": "message", "content": "Working..."}
{"type": "done", "session_id": "abc-123"}
"#;

        let sid = agent.parse_session_id(output);
        assert_eq!(sid, Some("abc-123".to_string()));
    }

    #[test]
    fn test_parse_session_id_invalid_json() {
        let agent = ClaudeAgent::default();
        let output = "This is not JSON";

        let sid = agent.parse_session_id(output);
        assert_eq!(sid, None);
    }

    #[test]
    fn test_parse_session_id_missing_field() {
        let agent = ClaudeAgent::default();
        let output = r#"{"result": "done"}"#;

        let sid = agent.parse_session_id(output);
        assert_eq!(sid, None);
    }

    // --- Availability Tests ---

    #[test]
    fn test_is_available_default_binary() {
        let agent = ClaudeAgent::default();
        // This will depend on environment
        // In CI, we may need to mock this
        let _ = agent.is_available();
    }

    #[test]
    fn test_is_available_missing_binary() {
        let agent = ClaudeAgent::new("nonexistent-binary-xyz");
        assert!(!agent.is_available());
    }
}
```

### Integration Tests

```rust
// tests/integration/headless_test.rs

#[tokio::test]
#[ignore] // Requires claude CLI installed
async fn test_headless_simple_prompt() {
    let agent = ClaudeAgent::default();
    let temp_dir = TempDir::new().unwrap();

    // Initialize a git repo in temp dir
    init_git_repo(temp_dir.path()).await;

    let options = HeadlessOptions::default();
    let result = agent.run_headless(
        "What is 2 + 2? Reply with just the number.",
        temp_dir.path(),
        &options,
    ).await.unwrap();

    assert!(result.success);
    assert!(result.session_id.is_some());
    assert!(result.stdout.contains("4") || result.result.map(|r| r.contains("4")).unwrap_or(false));
}

#[tokio::test]
#[ignore]
async fn test_headless_with_timeout() {
    let agent = ClaudeAgent::default();
    let temp_dir = TempDir::new().unwrap();
    init_git_repo(temp_dir.path()).await;

    let options = HeadlessOptions {
        timeout_secs: Some(1), // Very short timeout
        ..Default::default()
    };

    let result = agent.run_headless(
        "Write a 10000 word essay about the history of computing.",
        temp_dir.path(),
        &options,
    ).await;

    assert!(matches!(result, Err(AgentError::Timeout { seconds: 1 })));
}

#[tokio::test]
#[ignore]
async fn test_session_resume_flow() {
    let agent = ClaudeAgent::default();
    let temp_dir = TempDir::new().unwrap();
    init_git_repo(temp_dir.path()).await;

    // First prompt
    let result1 = agent.run_headless(
        "Remember the number 42.",
        temp_dir.path(),
        &HeadlessOptions::default(),
    ).await.unwrap();

    let session_id = result1.session_id.expect("Should have session ID");

    // Second prompt with resume
    let options = HeadlessOptions {
        extra_flags: vec![
            "--resume".to_string(),
            session_id.clone(),
        ],
        ..Default::default()
    };

    let result2 = agent.run_headless(
        "What number did I ask you to remember?",
        temp_dir.path(),
        &options,
    ).await.unwrap();

    assert!(result2.success);
    assert!(result2.stdout.contains("42") || result2.result.map(|r| r.contains("42")).unwrap_or(false));
}
```

### End-to-End Tests

```rust
// tests/e2e/create_with_prompt_test.rs

#[tokio::test]
#[ignore]
async fn test_create_shard_with_prompt_e2e() {
    let test_repo = setup_test_repo().await;

    // Run shards create with prompt
    let output = Command::new("cargo")
        .args(["run", "--", "create", "test-shard",
               "--agent", "claude",
               "--prompt", "Create a file called hello.txt with 'Hello World'"])
        .current_dir(&test_repo)
        .output()
        .await
        .unwrap();

    assert!(output.status.success());

    // Verify worktree was created
    let worktree_path = test_repo.join(".shards/test-shard");
    assert!(worktree_path.exists());

    // Verify file was created by Claude
    let hello_file = worktree_path.join("hello.txt");
    assert!(hello_file.exists());

    // Verify session has agent_session_id
    let session = load_session("test-shard").await.unwrap();
    assert!(session.agent_session_id.is_some());
    assert_eq!(session.initial_prompt, Some("Create a file called hello.txt with 'Hello World'".to_string()));

    // Cleanup
    cleanup_test_repo(test_repo).await;
}

#[tokio::test]
async fn test_create_shard_prompt_from_stdin() {
    let test_repo = setup_test_repo().await;

    let mut child = Command::new("cargo")
        .args(["run", "--", "create", "stdin-test",
               "--agent", "claude", "--prompt-stdin"])
        .current_dir(&test_repo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    // Write prompt to stdin
    let stdin = child.stdin.as_mut().unwrap();
    stdin.write_all(b"What is 1 + 1?").await.unwrap();
    drop(stdin);

    let output = child.wait_with_output().await.unwrap();
    assert!(output.status.success());

    cleanup_test_repo(test_repo).await;
}

#[tokio::test]
async fn test_send_prompt_to_existing_shard() {
    let test_repo = setup_test_repo().await;

    // Create shard first
    Command::new("cargo")
        .args(["run", "--", "create", "send-test",
               "--agent", "claude", "--no-terminal"])
        .current_dir(&test_repo)
        .output()
        .await
        .unwrap();

    // Send prompt
    let output = Command::new("cargo")
        .args(["run", "--", "send", "send-test",
               "Create a README.md file"])
        .current_dir(&test_repo)
        .output()
        .await
        .unwrap();

    assert!(output.status.success());

    // Verify README was created
    let readme = test_repo.join(".shards/send-test/README.md");
    assert!(readme.exists());

    cleanup_test_repo(test_repo).await;
}
```

### Mock Tests (No Claude Required)

```rust
// crates/shards-core/src/agents/mock.rs

#[cfg(test)]
pub struct MockClaudeAgent {
    pub should_succeed: bool,
    pub session_id: Option<String>,
    pub output: String,
}

#[cfg(test)]
impl MockClaudeAgent {
    pub fn success() -> Self {
        Self {
            should_succeed: true,
            session_id: Some("mock-session-123".to_string()),
            output: r#"{"session_id": "mock-session-123", "result": "Done"}"#.to_string(),
        }
    }

    pub fn failure() -> Self {
        Self {
            should_succeed: false,
            session_id: None,
            output: "Error: Something went wrong".to_string(),
        }
    }
}

#[cfg(test)]
#[async_trait]
impl AgentRunner for MockClaudeAgent {
    fn name(&self) -> &'static str { "claude" }
    fn display_name(&self) -> &'static str { "Claude Code (Mock)" }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            supports_headless: true,
            supports_resume: true,
            supports_json_output: true,
            supports_stdin_pipe: true,
            provides_session_id: true,
        }
    }

    fn is_available(&self) -> bool { true }
    fn binary(&self) -> &str { "claude" }

    async fn run_headless(
        &self,
        _prompt: &str,
        _working_dir: &Path,
        _options: &HeadlessOptions,
    ) -> Result<HeadlessResult, AgentError> {
        if self.should_succeed {
            Ok(HeadlessResult {
                success: true,
                exit_code: 0,
                session_id: self.session_id.clone(),
                stdout: self.output.clone(),
                stderr: String::new(),
                result: Some("Done".to_string()),
            })
        } else {
            Err(AgentError::HeadlessFailed {
                message: "Mock failure".to_string(),
                exit_code: 1,
            })
        }
    }

    fn build_resume_command(&self, session_id: &str) -> Option<String> {
        Some(format!("claude --resume {}", session_id))
    }

    fn build_interactive_command(&self) -> String {
        "claude".to_string()
    }

    fn parse_session_id(&self, _stdout: &str) -> Option<String> {
        self.session_id.clone()
    }
}
```

---

## Edge Cases

### Edge Case Matrix

| # | Scenario | Expected Behavior | Test |
|---|----------|-------------------|------|
| 1 | Empty prompt string | Fail with `EmptyPrompt` error | `test_empty_prompt_error` |
| 2 | Prompt file doesn't exist | Fail with `PromptFileError` | `test_missing_prompt_file` |
| 3 | Prompt file is empty | Fail with `EmptyPrompt` error | `test_empty_prompt_file` |
| 4 | Stdin is empty | Fail with `EmptyPrompt` error | `test_empty_stdin` |
| 5 | Claude binary not in PATH | Fail with `AgentNotInstalled` | `test_claude_not_installed` |
| 6 | Headless times out | Fail with `HeadlessTimeout` | `test_headless_timeout` |
| 7 | Headless returns non-zero | Log warning, open terminal anyway | `test_headless_failure_continues` |
| 8 | JSON output is malformed | Continue without session_id | `test_malformed_json_output` |
| 9 | No session_id in output | Open fresh interactive session | `test_missing_session_id` |
| 10 | Worktree creation fails | Fail with existing worktree error | `test_worktree_already_exists` |
| 11 | Terminal spawn fails | Fail with TerminalError | `test_terminal_spawn_failure` |
| 12 | `--no-terminal` with prompt | Run headless only, don't open terminal | `test_no_terminal_flag` |
| 13 | Very long prompt (>100KB) | Should work (Claude handles) | `test_long_prompt` |
| 14 | Prompt with special characters | Properly escaped | `test_special_chars_prompt` |
| 15 | Prompt with newlines | Preserved correctly | `test_multiline_prompt` |
| 16 | Unicode prompt | Handled correctly | `test_unicode_prompt` |
| 17 | `shards send` to non-existent shard | Fail with `SessionNotFound` | `test_send_nonexistent` |
| 18 | `shards send` to shard without session_id | Create new session | `test_send_no_session_id` |
| 19 | Session JSON is corrupted | Fail with parse error, suggest cleanup | `test_corrupted_session` |
| 20 | Concurrent creates with same name | First wins, second fails | `test_concurrent_create` |

### Edge Case Implementations

```rust
// tests/edge_cases.rs

#[test]
fn test_empty_prompt_error() {
    let options = CreateSessionOptions {
        prompt: Some("".to_string()),
        ..Default::default()
    };

    let result = resolve_prompt(&options).await;
    assert!(matches!(result, Err(SessionError::EmptyPrompt { .. })));
}

#[test]
fn test_whitespace_only_prompt_error() {
    let options = CreateSessionOptions {
        prompt: Some("   \n\t  ".to_string()),
        ..Default::default()
    };

    let result = resolve_prompt(&options).await;
    assert!(matches!(result, Err(SessionError::EmptyPrompt { .. })));
}

#[test]
fn test_special_chars_prompt() {
    let prompt = r#"Fix the bug where "quotes" and 'apostrophes' cause issues with $variables and `backticks`"#;
    let agent = ClaudeAgent::default();

    // Should not panic or error during command building
    let cmd = format!("claude -p {:?} --output-format json", prompt);
    assert!(cmd.contains("quotes"));
}

#[tokio::test]
async fn test_headless_failure_still_opens_terminal() {
    let mut registry = AgentRegistry::new();
    registry.register(Box::new(MockClaudeAgent::failure()));

    let options = CreateSessionOptions {
        prompt: Some("This will fail".to_string()),
        no_terminal: false,
        ..Default::default()
    };

    let result = create_session("test", "claude", options, &config, &registry).await;

    // Should succeed (terminal opened) but with warning logged
    assert!(result.is_ok());
    assert_eq!(result.unwrap().headless_success, Some(false));
}

#[test]
fn test_prompt_priority() {
    // stdin > file > arg
    let options = CreateSessionOptions {
        prompt: Some("arg prompt".to_string()),
        prompt_file: Some(PathBuf::from("file.txt")),
        prompt_stdin: true,
        ..Default::default()
    };

    // stdin should take priority
    // (test with mock stdin)
}
```

---

## Future Work: Other CLIs

### Phase 2: OpenAI Codex CLI

```rust
// crates/shards-core/src/agents/codex.rs (future)

pub struct CodexAgent { /* ... */ }

impl AgentRunner for CodexAgent {
    fn name(&self) -> &'static str { "codex" }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            supports_headless: true,     // codex exec
            supports_resume: true,       // --last
            supports_json_output: true,  // --json
            supports_stdin_pipe: true,   // -
            provides_session_id: true,
        }
    }

    async fn run_headless(&self, prompt: &str, working_dir: &Path, options: &HeadlessOptions)
        -> Result<HeadlessResult, AgentError>
    {
        // codex exec --json "prompt"
        todo!()
    }

    fn build_resume_command(&self, _session_id: &str) -> Option<String> {
        Some("codex --last".to_string())  // Codex uses --last, not session ID
    }
}
```

### Phase 3: Gemini CLI

```rust
// crates/shards-core/src/agents/gemini.rs (future)

pub struct GeminiAgent { /* ... */ }

impl AgentRunner for GeminiAgent {
    fn name(&self) -> &'static str { "gemini" }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            supports_headless: true,     // -p
            supports_resume: true,       // /chat resume
            supports_json_output: true,  // --output json
            supports_stdin_pipe: true,
            provides_session_id: true,
        }
    }
}
```

### Phase 4: Aider (Stateless)

```rust
// crates/shards-core/src/agents/aider.rs (future)

pub struct AiderAgent { /* ... */ }

impl AgentRunner for AiderAgent {
    fn name(&self) -> &'static str { "aider" }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            supports_headless: true,     // --message
            supports_resume: false,      // STATELESS
            supports_json_output: false,
            supports_stdin_pipe: true,   // --message-file
            provides_session_id: false,
        }
    }

    fn build_resume_command(&self, _session_id: &str) -> Option<String> {
        None  // Aider doesn't support resume
    }
}
```

### Phase 5: Kiro CLI

```rust
// crates/shards-core/src/agents/kiro.rs (future)

pub struct KiroAgent { /* ... */ }

// Implementation TBD - research needed on Kiro's headless capabilities
```

---

## Implementation Phases

### Phase 1: Foundation (This PR)

**Goal**: Claude Code support with full feature set

| Task | Effort | Priority |
|------|--------|----------|
| Create `src/agents/` module structure | S | P0 |
| Implement `AgentRunner` trait | M | P0 |
| Implement `AgentRegistry` | S | P0 |
| Implement `ClaudeAgent` | M | P0 |
| Add new CLI flags to `create` command | S | P0 |
| Update session handler for headless flow | L | P0 |
| Add `agent_session_id` to Session struct | S | P0 |
| Implement `shards send` command | M | P1 |
| Implement `shards attach` command | S | P1 |
| Implement `shards agents` command | S | P2 |
| Unit tests for ClaudeAgent | M | P0 |
| Integration tests (requires claude) | M | P1 |
| Update documentation | S | P1 |

**Estimated effort**: 3-5 days

### Phase 2: Additional Agents

| Agent | Effort | Notes |
|-------|--------|-------|
| CodexAgent | M | Similar to Claude, well-documented |
| GeminiAgent | M | Similar to Claude |
| AiderAgent | S | Simpler (stateless) |
| KiroAgent | L | Needs more research |

### Phase 3: Advanced Features

- Streaming output during headless execution
- Multiple prompts in sequence before opening terminal
- Prompt templates / aliases
- Agent configuration in shards config file

---

## Open Questions

1. **Should headless failure block terminal opening?**
   - Current plan: No, log warning and open terminal anyway
   - User can debug in interactive mode
   - Configurable via flag?

2. **How to handle session ID changes across prompts?**
   - Some agents may return different session IDs for each prompt
   - Should we track all of them or just the latest?

3. **Should `shards send` require the shard to be in "suspended" state?**
   - Or can we send to any shard regardless of terminal state?
   - May interfere with running interactive session

4. **Timeout defaults?**
   - No timeout by default (let agent run to completion)?
   - Or a generous default like 10 minutes?

5. **Output storage?**
   - Should we store headless output in session metadata?
   - Or write to a log file in the worktree?
   - Could get large for long outputs

6. **Async runtime?**
   - Currently Shards is sync
   - Headless execution benefits from async (timeout, concurrent operations)
   - Migrate to tokio? Or use blocking in separate thread?

---

## Appendix: Claude Code CLI Reference

### Headless Mode Flags

```
-p, --print <PROMPT>         Run prompt and exit (no REPL)
--output-format <FORMAT>     Output format: text, json, stream-json
--allowedTools <TOOLS>       Pre-approve specific tools
--resume <SESSION_ID>        Resume a previous session
--continue                   Continue most recent session
--max-turns <N>              Limit agent turns
--max-budget-usd <USD>       Limit cost
```

### JSON Output Structure

```json
{
  "session_id": "uuid",
  "result": "...",
  "cost": {
    "input_tokens": 1234,
    "output_tokens": 567,
    "total_usd": 0.05
  },
  "turns": 3,
  "tools_used": ["Read", "Edit", "Bash"]
}
```

### Session Persistence

- Sessions stored in `~/.claude/sessions/`
- Sessions are per-project (git repo root)
- Worktrees share session visibility within same repo
- Sessions can degrade after 3-4 days of heavy use
