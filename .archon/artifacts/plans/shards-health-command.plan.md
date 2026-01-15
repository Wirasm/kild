# Feature: Shards Health Command

## Summary

Implement a comprehensive `shards health` CLI command that provides dashboard-style visibility into all active AI agent sessions with real-time health metrics including process status, CPU usage, memory consumption, and activity tracking. The command supports multiple output formats (table, JSON) and filtering options (all projects, specific shard, watch mode).

## User Story

As a developer managing multiple AI agent sessions
I want to see a dashboard of all active shards with health metrics
So that I can quickly identify which agents are working, idle, or stuck without checking each terminal

## Problem Statement

Currently, users have no visibility into the health and activity status of their running AI agent sessions. The existing `shards list` command shows basic session information but lacks:
- Process health monitoring (running/stopped/crashed)
- Resource usage metrics (CPU, memory)
- Activity tracking (last user input or agent output)
- Status classification (working/idle/stuck)
- JSON output for programmatic consumption
- Per-shard detailed views

This makes it difficult to troubleshoot stuck agents, identify resource-heavy sessions, or understand which agents are actively working vs idle.

## Solution Statement

Create a new `health` subcommand with multiple modes:
1. **Default mode** (`shards health`): Table view of all shards in current project with health metrics
2. **Specific shard** (`shards health <branch>`): Detailed health view for one shard
3. **All projects** (`shards health --all`): Cross-project health dashboard
4. **JSON output** (`shards health --json`): Machine-readable format for UI consumption
5. **Watch mode** (`shards health --watch`): Continuous refresh (future enhancement)

The implementation builds on existing PID tracking, extends it with sysinfo-based resource monitoring, and adds activity timestamp tracking to session persistence.

## Metadata

| Field            | Value                                             |
| ---------------- | ------------------------------------------------- |
| Type             | NEW_CAPABILITY                                    |
| Complexity       | MEDIUM                                            |
| Systems Affected | CLI commands, session tracking, process monitoring, terminal output formatting |
| Dependencies     | sysinfo 0.37.2 (already in Cargo.toml), chrono 0.4, serde_json 1.0 |
| Estimated Tasks  | 12                                                |

---

## UX Design

### Before State
```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   User runs: shards list                                                      ║
║   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐            ║
║   │   CLI       │ ──────► │  Sessions   │ ──────► │   Table     │            ║
║   │   Parser    │         │  Handler    │         │   Output    │            ║
║   └─────────────┘         └─────────────┘         └─────────────┘            ║
║                                                                               ║
║   USER_FLOW:                                                                  ║
║   1. User runs `shards list`                                                  ║
║   2. Sees basic info: branch, agent, status, created time, port range         ║
║   3. Process status shows Run/Stop/Err but no health details                  ║
║   4. No CPU/memory usage visible                                              ║
║   5. No activity tracking - can't tell if agent is stuck                      ║
║   6. No JSON output option for programmatic use                               ║
║                                                                               ║
║   PAIN_POINT:                                                                 ║
║   - Can't identify stuck agents (working but no activity for 10+ minutes)    ║
║   - No resource usage visibility (which agent is using high CPU?)             ║
║   - Can't filter by specific shard or see all projects                        ║
║   - No machine-readable output for building UIs                               ║
║                                                                               ║
║   DATA_FLOW: CLI → load sessions from files → format table → print           ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝

╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   User runs: shards health [options]                                          ║
║   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐            ║
║   │   CLI       │ ──────► │  Health     │ ──────► │  Formatted  │            ║
║   │   Parser    │         │  Handler    │         │  Output     │            ║
║   └─────────────┘         └─────────────┘         └─────────────┘            ║
║          │                       │                       │                    ║
║          │                       ▼                       │                    ║
║          │              ┌─────────────────┐              │                    ║
║          │              │  Process        │              │                    ║
║          │              │  Monitor        │              │                    ║
║          │              │  (sysinfo)      │              │                    ║
║          │              └─────────────────┘              │                    ║
║          │                       │                       │                    ║
║          │                       ▼                       │                    ║
║          │              ┌─────────────────┐              │                    ║
║          │              │  Activity       │              │                    ║
║          │              │  Tracker        │              │                    ║
║          │              └─────────────────┘              │                    ║
║          │                                               │                    ║
║          └──────────────► Format: Table or JSON ◄────────┘                    ║
║                                                                               ║
║   USER_FLOW:                                                                  ║
║   1. User runs `shards health` (default: current project table view)          ║
║   2. Sees comprehensive health dashboard with:                                ║
║      - Process status: Running/Idle/Stuck/Crashed                             ║
║      - CPU usage percentage                                                   ║
║      - Memory usage (MB)                                                      ║
║      - Last activity timestamp                                                ║
║      - Status indicators (✅ working, ⏸️  idle, ⚠️  stuck, ❌ crashed)         ║
║   3. User runs `shards health <branch>` for detailed single-shard view        ║
║   4. User runs `shards health --json` to get machine-readable output          ║
║   5. User runs `shards health --all` to see all projects                      ║
║   6. (Future) User runs `shards health --watch` for continuous refresh        ║
║                                                                               ║
║   VALUE_ADD:                                                                  ║
║   - Instantly identify stuck agents (idle >10min with last message from user) ║
║   - See resource usage to find CPU/memory hogs                                ║
║   - JSON output enables building GUI dashboards                               ║
║   - Activity tracking shows when agent last did something                     ║
║                                                                               ║
║   DATA_FLOW:                                                                  ║
║   CLI → load sessions → enrich with process metrics (sysinfo) →               ║
║   calculate status (working/idle/stuck) → format (table/JSON) → output        ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location        | Before                          | After                                    | User_Action           | Impact                                      |
| --------------- | ------------------------------- | ---------------------------------------- | --------------------- | ------------------------------------------- |
| CLI             | `shards list`                   | `shards health`                          | Run new command       | See health metrics, not just basic info     |
| CLI             | No per-shard details            | `shards health <branch>`                 | Query specific shard  | Get detailed health view for one shard      |
| CLI             | No JSON output                  | `shards health --json`                   | Add --json flag       | Get machine-readable output for UIs         |
| CLI             | Current project only            | `shards health --all`                    | Add --all flag        | See all projects' shards                    |
| Session data    | No activity tracking            | `last_activity` timestamp in session     | Automatic tracking    | Know when agent last did something          |
| Process monitor | Basic PID check (running/not)   | CPU%, memory, detailed status            | Automatic enrichment  | See resource usage and health               |
| Status display  | Run/Stop/Err                    | Working/Idle/Stuck/Crashed with icons    | Visual indicators     | Quickly identify problem agents             |

---
## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `src/cli/commands.rs` | 1-100 | Pattern to MIRROR for command handlers - shows logging, error handling, table formatting |
| P0 | `src/cli/app.rs` | 1-150 | Pattern to MIRROR for adding new subcommand with clap |
| P0 | `src/sessions/types.rs` | 1-120 | Session struct to EXTEND with activity tracking |
| P0 | `src/process/operations.rs` | 1-150 | Process monitoring patterns to USE for health checks |
| P1 | `src/sessions/handler.rs` | 1-250 | Handler/Operations pattern to FOLLOW |
| P1 | `src/sessions/operations.rs` | 140-200 | JSON serialization pattern to MIRROR |
| P1 | `src/sessions/errors.rs` | 1-100 | Error handling pattern to EXTEND |
| P2 | `src/core/logging.rs` | 1-30 | Logging setup to USE |

**External Documentation:**

| Source | Section | Why Needed |
|--------|---------|------------|
| [sysinfo docs v0.37.2](https://docs.rs/sysinfo/0.37.2/) | System, Process, Cpu structs | CPU and memory monitoring implementation |
| [serde_json docs](https://docs.rs/serde_json/1.0/) | Serialization | JSON output formatting |

---

## Patterns to Mirror

**CLI_COMMAND_STRUCTURE:**
```rust
// SOURCE: src/cli/app.rs:40-60
// COPY THIS PATTERN for adding health subcommand:
.subcommand(
    Command::new("status")
        .about("Show detailed status of a shard")
        .arg(
            Arg::new("branch")
                .help("Branch name of the shard to check")
                .required(true)
                .index(1)
        )
)
```

**COMMAND_HANDLER_PATTERN:**
```rust
// SOURCE: src/cli/commands.rs:10-30
// COPY THIS PATTERN for health command handler:
pub fn run_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    events::log_app_startup();

    match matches.subcommand() {
        Some(("create", sub_matches)) => handle_create_command(sub_matches),
        Some(("list", _)) => handle_list_command(),
        // ADD: Some(("health", sub_matches)) => handle_health_command(sub_matches),
        _ => {
            error!(event = "cli.command_unknown");
            Err("Unknown command".into())
        }
    }
}
```

**TABLE_FORMATTING_PATTERN:**
```rust
// SOURCE: src/cli/commands.rs:80-120
// COPY THIS PATTERN for health table output:
println!("Active shards:");
println!("┌──────────────────┬─────────┬─────────┬─────────────────────┬─────────────┬─────────────┐");
println!("│ Branch           │ Agent   │ Status  │ Created             │ Port Range  │ Process     │");
println!("├──────────────────┼─────────┼─────────┼─────────────────────┼─────────────┼─────────────┤");

for session in &sessions {
    println!(
        "│ {:<16} │ {:<7} │ {:<7} │ {:<19} │ {:<11} │ {:<11} │",
        truncate(&session.branch, 16),
        truncate(&session.agent, 7),
        format!("{:?}", session.status).to_lowercase(),
        truncate(&session.created_at, 19),
        truncate(&port_range, 11),
        truncate(&process_status, 11)
    );
}

println!("└──────────────────┴─────────┴─────────┴─────────────────────┴─────────────┘");
```

**TRUNCATE_HELPER:**
```rust
// SOURCE: src/cli/commands.rs:200-210
// COPY THIS PATTERN for table cell truncation:
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        format!("{:<width$}", s, width = max_len)
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}
```

**PROCESS_MONITORING_PATTERN:**
```rust
// SOURCE: src/process/operations.rs:10-30
// COPY THIS PATTERN for health checks:
pub fn is_process_running(pid: u32) -> Result<bool, ProcessError> {
    let mut system = System::new();
    let pid_obj = SysinfoPid::from_u32(pid);
    system.refresh_processes(ProcessesToUpdate::Some(&[pid_obj]), true);
    Ok(system.process(pid_obj).is_some())
}

pub fn get_process_info(pid: u32) -> Result<ProcessInfo, ProcessError> {
    let mut system = System::new();
    let pid_obj = SysinfoPid::from_u32(pid);
    system.refresh_processes(ProcessesToUpdate::Some(&[pid_obj]), true);

    match system.process(pid_obj) {
        Some(process) => Ok(ProcessInfo {
            pid: Pid::from_raw(pid),
            name: process.name().to_string_lossy().to_string(),
            status: ProcessStatus::from(process.status()),
            start_time: process.start_time(),
        }),
        None => Err(ProcessError::NotFound { pid }),
    }
}
```

**JSON_SERIALIZATION_PATTERN:**
```rust
// SOURCE: src/sessions/operations.rs:148-150
// COPY THIS PATTERN for JSON output:
let session_json = serde_json::to_string_pretty(session)
    .map_err(|e| SessionError::IoError { 
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, e) 
    })?;
```

**LOGGING_PATTERN:**
```rust
// SOURCE: src/sessions/handler.rs:15-25
// COPY THIS PATTERN for health command logging:
info!(
    event = "session.create_started",
    branch = request.branch,
    agent = agent,
    command = agent_command
);

// ... operation ...

info!(
    event = "session.create_completed",
    session_id = session_id,
    branch = validated.name,
    agent = session.agent
);
```

**ERROR_HANDLING_PATTERN:**
```rust
// SOURCE: src/sessions/errors.rs:1-50
// COPY THIS PATTERN for health-specific errors:
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Session '{name}' not found")]
    NotFound { name: String },
    
    #[error("IO operation failed: {source}")]
    IoError {
        #[from]
        source: std::io::Error,
    },
}

impl ShardsError for SessionError {
    fn error_code(&self) -> &'static str {
        match self {
            SessionError::NotFound { .. } => "SESSION_NOT_FOUND",
            SessionError::IoError { .. } => "IO_ERROR",
        }
    }
}
```

---
## Files to Change

| File                                      | Action | Justification                                                    |
| ----------------------------------------- | ------ | ---------------------------------------------------------------- |
| `src/cli/app.rs`                          | UPDATE | Add `health` subcommand with args (branch, --all, --json)       |
| `src/cli/commands.rs`                     | UPDATE | Add `handle_health_command` function                             |
| `src/sessions/types.rs`                   | UPDATE | Add `last_activity` field to Session struct                      |
| `src/health/mod.rs`                       | CREATE | Public API exports for health feature slice                      |
| `src/health/types.rs`                     | CREATE | HealthStatus, HealthMetrics, HealthOutput types                  |
| `src/health/errors.rs`                    | CREATE | Health-specific error types                                      |
| `src/health/operations.rs`                | CREATE | Pure logic: status calculation, metric enrichment                |
| `src/health/handler.rs`                   | CREATE | I/O orchestration: load sessions, gather metrics, format output  |
| `src/lib.rs`                              | UPDATE | Add `pub mod health;` to expose health module                    |
| `src/process/operations.rs`               | UPDATE | Add `get_process_metrics` function for CPU/memory                |
| `src/process/types.rs`                    | UPDATE | Add `ProcessMetrics` struct with cpu_usage, memory_usage         |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **Watch mode** (`--watch` flag): Deferred to future iteration - requires continuous refresh loop and terminal clearing logic
- **Historical activity tracking**: Only tracking last activity timestamp, not full activity history
- **Process tree visualization**: Not showing child processes or process hierarchy
- **Network usage metrics**: Only CPU and memory, no network I/O tracking
- **Disk I/O metrics**: Not tracking disk read/write operations
- **Custom alert thresholds**: No configurable thresholds for stuck/high-CPU warnings
- **Activity log parsing**: Not parsing terminal output to detect activity - relying on timestamp updates
- **Multi-user support**: Health view is per-user, not showing other users' shards
- **Remote shard monitoring**: Only local shards, no remote machine monitoring

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: UPDATE `src/sessions/types.rs` - Add activity tracking

- **ACTION**: ADD `last_activity` field to Session struct
- **IMPLEMENT**: 
  ```rust
  pub struct Session {
      // ... existing fields ...
      
      /// Timestamp of last detected activity (user input or agent output)
      /// Used to determine if session is idle or stuck
      #[serde(default)]
      pub last_activity: Option<String>,
  }
  ```
- **MIRROR**: `src/sessions/types.rs:10-50` - follow existing field pattern with serde attributes
- **IMPORTS**: None needed (chrono already imported)
- **GOTCHA**: Use `#[serde(default)]` to handle backward compatibility with existing session files
- **VALIDATE**: `cargo check --lib`

### Task 2: CREATE `src/health/types.rs` - Define health data structures

- **ACTION**: CREATE health-specific types
- **IMPLEMENT**:
  ```rust
  use serde::{Deserialize, Serialize};
  
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  pub enum HealthStatus {
      Working,    // Process running, recent activity
      Idle,       // Process running, no activity >10min, last message from agent
      Stuck,      // Process running, no activity >10min, last message from user
      Crashed,    // Process not running but session exists
      Unknown,    // Cannot determine status
  }
  
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct HealthMetrics {
      pub cpu_usage_percent: Option<f32>,
      pub memory_usage_mb: Option<u64>,
      pub process_status: String,
      pub last_activity: Option<String>,
      pub status: HealthStatus,
  }
  
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct ShardHealth {
      pub session_id: String,
      pub project_id: String,
      pub branch: String,
      pub agent: String,
      pub worktree_path: String,
      pub created_at: String,
      pub metrics: HealthMetrics,
  }
  
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct HealthOutput {
      pub shards: Vec<ShardHealth>,
      pub total_count: usize,
      pub working_count: usize,
      pub idle_count: usize,
      pub stuck_count: usize,
      pub crashed_count: usize,
  }
  ```
- **MIRROR**: `src/sessions/types.rs:10-80` - follow Session struct pattern
- **IMPORTS**: `use serde::{Deserialize, Serialize};`
- **VALIDATE**: `cargo check --lib`

### Task 3: CREATE `src/health/errors.rs` - Define error types

- **ACTION**: CREATE health-specific errors
- **IMPLEMENT**:
  ```rust
  use crate::core::errors::ShardsError;
  
  #[derive(Debug, thiserror::Error)]
  pub enum HealthError {
      #[error("Failed to gather health metrics: {message}")]
      MetricsGatherFailed { message: String },
      
      #[error("Session error: {source}")]
      SessionError {
          #[from]
          source: crate::sessions::errors::SessionError,
      },
      
      #[error("Process error: {source}")]
      ProcessError {
          #[from]
          source: crate::process::errors::ProcessError,
      },
      
      #[error("IO operation failed: {source}")]
      IoError {
          #[from]
          source: std::io::Error,
      },
  }
  
  impl ShardsError for HealthError {
      fn error_code(&self) -> &'static str {
          match self {
              HealthError::MetricsGatherFailed { .. } => "HEALTH_METRICS_FAILED",
              HealthError::SessionError { .. } => "HEALTH_SESSION_ERROR",
              HealthError::ProcessError { .. } => "HEALTH_PROCESS_ERROR",
              HealthError::IoError { .. } => "HEALTH_IO_ERROR",
          }
      }
      
      fn is_user_error(&self) -> bool {
          false
      }
  }
  ```
- **MIRROR**: `src/sessions/errors.rs:1-100` - follow error pattern exactly
- **IMPORTS**: `use crate::core::errors::ShardsError;`, `use thiserror::Error;`
- **VALIDATE**: `cargo check --lib`

### Task 4: UPDATE `src/process/types.rs` - Add metrics struct

- **ACTION**: ADD ProcessMetrics struct for CPU/memory data
- **IMPLEMENT**:
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct ProcessMetrics {
      pub cpu_usage_percent: f32,
      pub memory_usage_bytes: u64,
      pub memory_usage_mb: u64,
  }
  ```
- **MIRROR**: `src/process/types.rs:40-60` - follow ProcessInfo pattern
- **IMPORTS**: Already has serde imports
- **VALIDATE**: `cargo check --lib`

### Task 5: UPDATE `src/process/operations.rs` - Add metrics gathering

- **ACTION**: ADD function to get CPU and memory metrics
- **IMPLEMENT**:
  ```rust
  use crate::process::types::ProcessMetrics;
  
  /// Get CPU and memory usage metrics for a process
  pub fn get_process_metrics(pid: u32) -> Result<ProcessMetrics, ProcessError> {
      let mut system = System::new();
      let pid_obj = SysinfoPid::from_u32(pid);
      
      // Refresh process info with CPU usage
      system.refresh_processes(ProcessesToUpdate::Some(&[pid_obj]), true);
      
      match system.process(pid_obj) {
          Some(process) => {
              let memory_bytes = process.memory();
              let memory_mb = memory_bytes / 1_024 / 1_024;
              
              Ok(ProcessMetrics {
                  cpu_usage_percent: process.cpu_usage(),
                  memory_usage_bytes: memory_bytes,
                  memory_usage_mb: memory_mb,
              })
          }
          None => Err(ProcessError::NotFound { pid }),
      }
  }
  ```
- **MIRROR**: `src/process/operations.rs:50-80` - follow get_process_info pattern
- **IMPORTS**: Add `use crate::process::types::ProcessMetrics;`
- **GOTCHA**: sysinfo's `cpu_usage()` returns percentage (0-100), `memory()` returns bytes
- **VALIDATE**: `cargo check --lib`

### Task 6: CREATE `src/health/operations.rs` - Pure health logic

- **ACTION**: CREATE pure functions for status calculation and metric enrichment
- **IMPLEMENT**:
  ```rust
  use chrono::{DateTime, Utc};
  use crate::health::types::{HealthStatus, HealthMetrics, ShardHealth, HealthOutput};
  use crate::sessions::types::Session;
  use crate::process::types::ProcessMetrics;
  
  const IDLE_THRESHOLD_MINUTES: i64 = 10;
  
  /// Calculate health status based on process state and activity
  pub fn calculate_health_status(
      process_running: bool,
      last_activity: Option<&str>,
      last_message_from_user: bool,
  ) -> HealthStatus {
      if !process_running {
          return HealthStatus::Crashed;
      }
      
      let Some(activity_str) = last_activity else {
          return HealthStatus::Unknown;
      };
      
      let Ok(activity_time) = DateTime::parse_from_rfc3339(activity_str) else {
          return HealthStatus::Unknown;
      };
      
      let now = Utc::now();
      let minutes_since_activity = (now - activity_time).num_minutes();
      
      if minutes_since_activity < IDLE_THRESHOLD_MINUTES {
          HealthStatus::Working
      } else if last_message_from_user {
          HealthStatus::Stuck
      } else {
          HealthStatus::Idle
      }
  }
  
  /// Enrich session with health metrics
  pub fn enrich_session_with_health(
      session: &Session,
      process_metrics: Option<ProcessMetrics>,
      process_running: bool,
  ) -> ShardHealth {
      let status = calculate_health_status(
          process_running,
          session.last_activity.as_deref(),
          false, // TODO: Track last message sender in future
      );
      
      let metrics = HealthMetrics {
          cpu_usage_percent: process_metrics.as_ref().map(|m| m.cpu_usage_percent),
          memory_usage_mb: process_metrics.as_ref().map(|m| m.memory_usage_mb),
          process_status: if process_running { "Running".to_string() } else { "Stopped".to_string() },
          last_activity: session.last_activity.clone(),
          status,
      };
      
      ShardHealth {
          session_id: session.id.clone(),
          project_id: session.project_id.clone(),
          branch: session.branch.clone(),
          agent: session.agent.clone(),
          worktree_path: session.worktree_path.display().to_string(),
          created_at: session.created_at.clone(),
          metrics,
      }
  }
  
  /// Aggregate health statistics
  pub fn aggregate_health_stats(shards: &[ShardHealth]) -> HealthOutput {
      let mut working = 0;
      let mut idle = 0;
      let mut stuck = 0;
      let mut crashed = 0;
      
      for shard in shards {
          match shard.metrics.status {
              HealthStatus::Working => working += 1,
              HealthStatus::Idle => idle += 1,
              HealthStatus::Stuck => stuck += 1,
              HealthStatus::Crashed => crashed += 1,
              HealthStatus::Unknown => {}
          }
      }
      
      HealthOutput {
          shards: shards.to_vec(),
          total_count: shards.len(),
          working_count: working,
          idle_count: idle,
          stuck_count: stuck,
          crashed_count: crashed,
      }
  }
  ```
- **MIRROR**: `src/sessions/operations.rs:1-100` - follow pure function pattern
- **IMPORTS**: `use chrono::{DateTime, Utc};`
- **GOTCHA**: Use `num_minutes()` not `minutes()` for duration calculation
- **VALIDATE**: `cargo check --lib`

### Task 7: CREATE `src/health/handler.rs` - I/O orchestration

- **ACTION**: CREATE handler to gather health data and format output
- **IMPLEMENT**:
  ```rust
  use tracing::{error, info, warn};
  use crate::core::config::Config;
  use crate::health::{errors::HealthError, operations, types::*};
  use crate::sessions;
  use crate::process;
  
  /// Get health status for all sessions in current project
  pub fn get_health_all_sessions() -> Result<HealthOutput, HealthError> {
      info!(event = "health.get_all_started");
      
      let sessions = sessions::handler::list_sessions()?;
      let mut shard_healths = Vec::new();
      
      for session in sessions {
          let shard_health = enrich_session_with_metrics(&session);
          shard_healths.push(shard_health);
      }
      
      let output = operations::aggregate_health_stats(&shard_healths);
      
      info!(
          event = "health.get_all_completed",
          total = output.total_count,
          working = output.working_count,
          idle = output.idle_count,
          stuck = output.stuck_count,
          crashed = output.crashed_count
      );
      
      Ok(output)
  }
  
  /// Get health status for a specific session
  pub fn get_health_single_session(branch: &str) -> Result<ShardHealth, HealthError> {
      info!(event = "health.get_single_started", branch = branch);
      
      let session = sessions::handler::get_session(branch)?;
      let shard_health = enrich_session_with_metrics(&session);
      
      info!(
          event = "health.get_single_completed",
          branch = branch,
          status = ?shard_health.metrics.status
      );
      
      Ok(shard_health)
  }
  
  /// Helper to enrich session with process metrics
  fn enrich_session_with_metrics(session: &sessions::types::Session) -> ShardHealth {
      let (process_metrics, process_running) = if let Some(pid) = session.process_id {
          match process::is_process_running(pid) {
              Ok(true) => {
                  let metrics = process::get_process_metrics(pid).ok();
                  (metrics, true)
              }
              Ok(false) => (None, false),
              Err(e) => {
                  warn!(
                      event = "health.process_check_failed",
                      pid = pid,
                      session_branch = &session.branch,
                      error = %e
                  );
                  (None, false)
              }
          }
      } else {
          (None, false)
      };
      
      operations::enrich_session_with_health(session, process_metrics, process_running)
  }
  ```
- **MIRROR**: `src/sessions/handler.rs:1-150` - follow handler pattern with logging
- **IMPORTS**: `use tracing::{error, info, warn};`
- **VALIDATE**: `cargo check --lib`

### Task 8: CREATE `src/health/mod.rs` - Public API

- **ACTION**: CREATE module exports
- **IMPLEMENT**:
  ```rust
  pub mod errors;
  pub mod handler;
  pub mod operations;
  pub mod types;
  
  // Re-export commonly used types
  pub use errors::HealthError;
  pub use handler::{get_health_all_sessions, get_health_single_session};
  pub use types::{HealthMetrics, HealthOutput, HealthStatus, ShardHealth};
  ```
- **MIRROR**: `src/sessions/mod.rs:1-10` - follow module pattern
- **VALIDATE**: `cargo check --lib`

### Task 9: UPDATE `src/lib.rs` - Expose health module

- **ACTION**: ADD health module to library exports
- **IMPLEMENT**: Add `pub mod health;` after other module declarations
- **MIRROR**: `src/lib.rs:1-20` - follow existing module pattern
- **VALIDATE**: `cargo check --lib`

### Task 10: UPDATE `src/cli/app.rs` - Add health subcommand

- **ACTION**: ADD health subcommand with arguments
- **IMPLEMENT**:
  ```rust
  .subcommand(
      Command::new("health")
          .about("Show health status and metrics for shards")
          .arg(
              Arg::new("branch")
                  .help("Branch name of specific shard to check (optional)")
                  .index(1)
          )
          .arg(
              Arg::new("all")
                  .long("all")
                  .help("Show health for all projects, not just current")
                  .action(clap::ArgAction::SetTrue)
          )
          .arg(
              Arg::new("json")
                  .long("json")
                  .help("Output in JSON format")
                  .action(clap::ArgAction::SetTrue)
          )
  )
  ```
- **MIRROR**: `src/cli/app.rs:40-80` - follow existing subcommand pattern
- **IMPORTS**: None needed (clap already imported)
- **GOTCHA**: Use `ArgAction::SetTrue` for boolean flags in clap 4.0
- **VALIDATE**: `cargo check --bin shards`

### Task 11: UPDATE `src/cli/commands.rs` - Add health command handler

- **ACTION**: ADD handle_health_command function and wire it up
- **IMPLEMENT**:
  ```rust
  // Add to match statement in run_command:
  Some(("health", sub_matches)) => handle_health_command(sub_matches),
  
  // Add new handler function:
  fn handle_health_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
      let branch = matches.get_one::<String>("branch");
      let show_all = matches.get_flag("all");
      let json_output = matches.get_flag("json");
      
      info!(
          event = "cli.health_started",
          branch = ?branch,
          show_all = show_all,
          json_output = json_output
      );
      
      if let Some(branch_name) = branch {
          // Single shard health
          match crate::health::get_health_single_session(branch_name) {
              Ok(shard_health) => {
                  if json_output {
                      println!("{}", serde_json::to_string_pretty(&shard_health)?);
                  } else {
                      print_single_shard_health(&shard_health);
                  }
                  
                  info!(event = "cli.health_completed", branch = branch_name);
                  Ok(())
              }
              Err(e) => {
                  eprintln!("❌ Failed to get health for shard '{}': {}", branch_name, e);
                  error!(event = "cli.health_failed", branch = branch_name, error = %e);
                  events::log_app_error(&e);
                  Err(e.into())
              }
          }
      } else {
          // All shards health
          match crate::health::get_health_all_sessions() {
              Ok(health_output) => {
                  if json_output {
                      println!("{}", serde_json::to_string_pretty(&health_output)?);
                  } else {
                      print_health_table(&health_output);
                  }
                  
                  info!(
                      event = "cli.health_completed",
                      total = health_output.total_count,
                      working = health_output.working_count
                  );
                  Ok(())
              }
              Err(e) => {
                  eprintln!("❌ Failed to get health status: {}", e);
                  error!(event = "cli.health_failed", error = %e);
                  events::log_app_error(&e);
                  Err(e.into())
              }
          }
      }
  }
  
  fn print_health_table(output: &crate::health::HealthOutput) {
      if output.shards.is_empty() {
          println!("No active shards found.");
          return;
      }
      
      println!("🏥 Shard Health Dashboard");
      println!("┌────┬──────────────────┬─────────┬──────────┬──────────┬──────────┬─────────────────────┐");
      println!("│ St │ Branch           │ Agent   │ CPU %    │ Memory   │ Status   │ Last Activity       │");
      println!("├────┼──────────────────┼─────────┼──────────┼──────────┼──────────┼─────────────────────┤");
      
      for shard in &output.shards {
          let status_icon = match shard.metrics.status {
              crate::health::HealthStatus::Working => "✅",
              crate::health::HealthStatus::Idle => "⏸️ ",
              crate::health::HealthStatus::Stuck => "⚠️ ",
              crate::health::HealthStatus::Crashed => "❌",
              crate::health::HealthStatus::Unknown => "❓",
          };
          
          let cpu_str = shard.metrics.cpu_usage_percent
              .map(|c| format!("{:.1}%", c))
              .unwrap_or_else(|| "N/A".to_string());
          
          let mem_str = shard.metrics.memory_usage_mb
              .map(|m| format!("{}MB", m))
              .unwrap_or_else(|| "N/A".to_string());
          
          let activity_str = shard.metrics.last_activity
              .as_ref()
              .map(|a| truncate(a, 19))
              .unwrap_or_else(|| "Never".to_string());
          
          println!(
              "│ {} │ {:<16} │ {:<7} │ {:<8} │ {:<8} │ {:<8} │ {:<19} │",
              status_icon,
              truncate(&shard.branch, 16),
              truncate(&shard.agent, 7),
              truncate(&cpu_str, 8),
              truncate(&mem_str, 8),
              truncate(&format!("{:?}", shard.metrics.status), 8),
              activity_str
          );
      }
      
      println!("└────┴──────────────────┴─────────┴──────────┴──────────┴──────────┴─────────────────────┘");
      println!();
      println!("Summary: {} total | {} working | {} idle | {} stuck | {} crashed",
          output.total_count,
          output.working_count,
          output.idle_count,
          output.stuck_count,
          output.crashed_count
      );
  }
  
  fn print_single_shard_health(shard: &crate::health::ShardHealth) {
      let status_icon = match shard.metrics.status {
          crate::health::HealthStatus::Working => "✅",
          crate::health::HealthStatus::Idle => "⏸️ ",
          crate::health::HealthStatus::Stuck => "⚠️ ",
          crate::health::HealthStatus::Crashed => "❌",
          crate::health::HealthStatus::Unknown => "❓",
      };
      
      println!("🏥 Shard Health: {}", shard.branch);
      println!("┌─────────────────────────────────────────────────────────────┐");
      println!("│ Branch:      {:<47} │", shard.branch);
      println!("│ Agent:       {:<47} │", shard.agent);
      println!("│ Status:      {} {:<44} │", status_icon, format!("{:?}", shard.metrics.status));
      println!("│ Created:     {:<47} │", shard.created_at);
      println!("│ Worktree:    {:<47} │", truncate(&shard.worktree_path, 47));
      
      if let Some(cpu) = shard.metrics.cpu_usage_percent {
          println!("│ CPU Usage:   {:<47} │", format!("{:.1}%", cpu));
      } else {
          println!("│ CPU Usage:   {:<47} │", "N/A");
      }
      
      if let Some(mem) = shard.metrics.memory_usage_mb {
          println!("│ Memory:      {:<47} │", format!("{} MB", mem));
      } else {
          println!("│ Memory:      {:<47} │", "N/A");
      }
      
      if let Some(activity) = &shard.metrics.last_activity {
          println!("│ Last Active: {:<47} │", truncate(activity, 47));
      } else {
          println!("│ Last Active: {:<47} │", "Never");
      }
      
      println!("└─────────────────────────────────────────────────────────────┘");
  }
  ```
- **MIRROR**: `src/cli/commands.rs:50-200` - follow existing command handler pattern
- **IMPORTS**: Add `use crate::health;` at top
- **GOTCHA**: Use `get_flag` not `get_one` for boolean flags in clap 4.0
- **VALIDATE**: `cargo check --bin shards && cargo build`

### Task 12: UPDATE session creation to set initial activity timestamp

- **ACTION**: UPDATE `src/sessions/handler.rs` create_session to set last_activity
- **IMPLEMENT**: In the Session creation block, add:
  ```rust
  last_activity: Some(chrono::Utc::now().to_rfc3339()),
  ```
- **MIRROR**: `src/sessions/handler.rs:80-100` - follow existing Session initialization
- **VALIDATE**: `cargo check --lib`

---
## Testing Strategy

### Unit Tests to Write

| Test File                                | Test Cases                                       | Validates                    |
| ---------------------------------------- | ------------------------------------------------ | ---------------------------- |
| `src/health/operations.rs`               | calculate_health_status with various inputs     | Status calculation logic     |
| `src/health/operations.rs`               | enrich_session_with_health                       | Metric enrichment            |
| `src/health/operations.rs`               | aggregate_health_stats                           | Statistics aggregation       |
| `src/process/operations.rs`              | get_process_metrics (integration test)           | CPU/memory gathering         |

### Edge Cases Checklist

- [ ] Session with no PID (process_id = None)
- [ ] Session with PID but process not running (PID reused)
- [ ] Session with no last_activity timestamp (backward compatibility)
- [ ] Session with invalid last_activity timestamp format
- [ ] Empty sessions list (no active shards)
- [ ] Process metrics unavailable (sysinfo fails)
- [ ] Very high CPU usage (>100% on multi-core)
- [ ] Very high memory usage (>1GB)
- [ ] Activity timestamp exactly at 10-minute threshold
- [ ] Activity timestamp in the future (clock skew)
- [ ] JSON output with special characters in branch names
- [ ] Table output with very long branch names (truncation)

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo check --lib && cargo check --bin shards && cargo clippy -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: UNIT_TESTS

```bash
cargo test health::operations::tests
cargo test process::operations::tests::test_process_lifecycle
```

**EXPECT**: All tests pass

### Level 3: FULL_SUITE

```bash
cargo test && cargo build --release
```

**EXPECT**: All tests pass, build succeeds, target/release/shards created

### Level 4: MANUAL_VALIDATION

**Test Scenario 1: Basic health command**
```bash
# Create a test shard
cargo run -- create test-health-branch --agent kiro

# Check health (should show Working status)
cargo run -- health

# Verify table output shows:
# - ✅ status icon
# - CPU and memory metrics
# - Recent last_activity timestamp
```

**Test Scenario 2: Single shard health**
```bash
# Get detailed health for specific shard
cargo run -- health test-health-branch

# Verify detailed view shows:
# - All session metadata
# - CPU/memory metrics
# - Last activity timestamp
```

**Test Scenario 3: JSON output**
```bash
# Get JSON output
cargo run -- health --json

# Verify:
# - Valid JSON structure
# - All fields present
# - Can be parsed by jq: cargo run -- health --json | jq '.total_count'
```

**Test Scenario 4: Crashed process detection**
```bash
# Create shard
cargo run -- create test-crash --agent kiro

# Kill the process manually (find PID from shards list)
kill -9 <PID>

# Check health (should show Crashed status)
cargo run -- health

# Verify:
# - ❌ status icon
# - Status shows "Crashed"
```

**Test Scenario 5: Backward compatibility**
```bash
# Test with existing session files (no last_activity field)
# Health command should handle gracefully with "Unknown" status
cargo run -- health
```

---

## Acceptance Criteria

- [ ] `shards health` displays table with all shards and health metrics
- [ ] `shards health <branch>` shows detailed view for specific shard
- [ ] `shards health --json` outputs valid JSON with all metrics
- [ ] `shards health --all` flag is recognized (implementation deferred)
- [ ] Health status correctly identifies: Working, Idle, Stuck, Crashed
- [ ] CPU usage percentage displayed (0-100%)
- [ ] Memory usage displayed in MB
- [ ] Last activity timestamp shown in table
- [ ] Status icons displayed: ✅ ⏸️  ⚠️  ❌ ❓
- [ ] Summary line shows counts by status
- [ ] Backward compatible with existing session files (no last_activity)
- [ ] Level 1-3 validation commands pass with exit 0
- [ ] All manual validation scenarios work as expected
- [ ] No regressions in existing `shards list` command
- [ ] Structured logging events for health operations

---

## Completion Checklist

- [ ] All 12 tasks completed in dependency order
- [ ] Each task validated immediately after completion
- [ ] Level 1: `cargo check && cargo clippy` passes
- [ ] Level 2: `cargo test health::` passes
- [ ] Level 3: `cargo test && cargo build --release` succeeds
- [ ] Level 4: All manual validation scenarios pass
- [ ] All acceptance criteria met
- [ ] Backward compatibility verified with existing sessions
- [ ] JSON output validated with jq
- [ ] Table formatting looks correct in terminal

---

## Risks and Mitigations

| Risk                                      | Likelihood | Impact | Mitigation                                                                 |
| ----------------------------------------- | ---------- | ------ | -------------------------------------------------------------------------- |
| sysinfo CPU metrics inaccurate on macOS   | MEDIUM     | LOW    | Document limitation, CPU% is best-effort metric                            |
| PID reuse causes incorrect process match  | LOW        | MEDIUM | Already mitigated with process_name and process_start_time validation      |
| Backward compat breaks existing sessions  | LOW        | HIGH   | Use `#[serde(default)]` on last_activity field                             |
| Activity tracking not implemented yet     | HIGH       | MEDIUM | Defer to future - use session creation time as initial activity            |
| Watch mode complexity                     | HIGH       | LOW    | Explicitly out of scope for this iteration                                 |
| JSON output breaks with special chars     | LOW        | LOW    | serde_json handles escaping automatically                                  |
| Table formatting breaks on narrow terms   | MEDIUM     | LOW    | Use truncate() helper, test with 80-column terminal                        |
| High memory usage on many sessions        | LOW        | LOW    | sysinfo is efficient, only refresh specific PIDs                           |

---

## Notes

### Design Decisions

**Why 10-minute idle threshold?**
- User specified this based on typical AI agent task duration
- Some operations (large builds, complex analysis) take >5 minutes
- 10 minutes balances between false positives and useful alerts

**Why not track last message sender (user vs agent)?**
- Requires PTY output parsing or terminal integration
- Deferred to future iteration when we have proper activity logging
- For now, assume all activity after 10min idle is "stuck" (conservative)

**Why separate Working/Idle/Stuck states?**
- Working: Agent is actively doing something (recent activity)
- Idle: Agent finished and waiting (no activity, but last output was from agent)
- Stuck: Agent appears frozen (no activity, last input was from user expecting response)
- This helps users quickly identify which agents need attention

**Why not use --watch flag yet?**
- Requires terminal clearing, refresh loop, signal handling
- Adds complexity without core value for MVP
- Users can manually re-run command or use `watch` command: `watch -n 5 shards health`

### Future Enhancements

1. **Activity tracking**: Integrate with terminal to detect actual user input / agent output
2. **Watch mode**: Built-in continuous refresh with `--watch` flag
3. **Alert thresholds**: Configurable thresholds for stuck/high-CPU warnings
4. **Historical metrics**: Track CPU/memory over time, show trends
5. **Process tree**: Show child processes spawned by agent
6. **Network I/O**: Track network usage for agents making API calls
7. **Disk I/O**: Track disk read/write for agents doing file operations
8. **Custom status rules**: User-defined rules for status classification
9. **Export metrics**: Export to Prometheus, CloudWatch, etc.
10. **Multi-user view**: Show all users' shards (requires permission model)

### Trade-offs

**Chose simplicity over accuracy for activity tracking:**
- Pro: Can ship feature without complex PTY integration
- Con: "Stuck" detection is conservative (may have false positives)
- Mitigation: Document limitation, improve in future iteration

**Chose table formatting over external library:**
- Pro: No new dependencies, full control over layout
- Con: More manual work, potential formatting bugs
- Mitigation: Reuse existing truncate() helper, test thoroughly

**Chose sysinfo over platform-specific APIs:**
- Pro: Cross-platform, well-maintained, already in dependencies
- Con: Some metrics may be less accurate than native APIs
- Mitigation: Document as best-effort metrics, good enough for dashboard

---

## Confidence Score

**8/10** for one-pass implementation success

**Rationale:**
- ✅ Clear patterns from existing codebase (cli/commands.rs, sessions/handler.rs)
- ✅ All dependencies already in Cargo.toml (sysinfo, serde_json, chrono)
- ✅ Well-defined types and error handling patterns to follow
- ✅ Comprehensive task breakdown with validation at each step
- ✅ Backward compatibility handled with serde defaults
- ⚠️  sysinfo CPU metrics behavior may vary by platform (needs testing)
- ⚠️  Table formatting edge cases (very long names, special chars)

**Confidence boosters:**
- Existing process monitoring code to build on
- Clear separation of pure logic (operations) and I/O (handler)
- JSON output is straightforward with serde
- Comprehensive validation plan with manual test scenarios

**Potential blockers:**
- sysinfo CPU usage accuracy on macOS (may need adjustment)
- Table column widths may need tuning after seeing real data
- Activity tracking is placeholder (but documented as future work)

---

## Next Step

To execute this plan, run:
```bash
# Start with Task 1 and proceed sequentially
# Validate after each task with the specified command
# Refer back to "Patterns to Mirror" section for code examples
```

Or use an implementation agent:
```bash
/execute-plan .archon/artifacts/plans/shards-health-command.plan.md
```
