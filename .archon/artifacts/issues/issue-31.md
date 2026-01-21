# Investigation: Add Enhanced Health Monitoring Features

**Issue**: #31 (https://github.com/Wirasm/shards/issues/31)
**Type**: ENHANCEMENT
**Investigated**: 2026-01-21T12:35:00Z

### Assessment

| Metric | Value | Reasoning |
|--------|-------|-----------|
| Priority | MEDIUM | Watch mode and configurable thresholds provide significant user value for monitoring long-running agent sessions; historical metrics is lower priority (P3 label) |
| Complexity | HIGH | 5+ files affected, requires new CLI flags, configuration schema extension, optional storage layer for metrics, and UI updates for watch mode |
| Confidence | HIGH | Clear existing architecture patterns found; configuration system (#30) is already implemented; activity tracking (#26) is already merged |

---

## Problem Statement

The health command provides basic functionality but lacks advanced features needed for production use: watch mode for continuous monitoring, configurable idle/stuck thresholds, historical metrics storage, and enhanced process monitoring. Users currently must manually re-run `shards health` to check status, cannot customize timeout thresholds, and have no way to track health trends over time.

---

## Analysis

### Change Rationale

The current health implementation in `src/health/` provides a solid foundation:
- `HealthStatus` enum (Working/Idle/Stuck/Crashed/Unknown) - `src/health/types.rs:4-10`
- `HealthMetrics` struct with CPU/memory - `src/health/types.rs:12-20`
- Status calculation with configurable threshold (already uses `AtomicI64`) - `src/health/operations.rs:7`
- JSON and table output formats - `src/cli/commands.rs:392-484`

The enhancement requires adding:
1. **Watch mode**: Continuous refresh loop with interval flag
2. **Threshold configuration**: Extend `ShardsConfig` with health section
3. **Historical storage**: Optional JSON/SQLite storage for metrics over time
4. **Enhanced process metrics**: Memory/CPU trends (requires periodic sampling)

### Dependencies Status

| Dependency | Status | Implication |
|------------|--------|-------------|
| #26 (last_activity tracking) | CLOSED/Merged | `last_activity` field exists in Session struct; health calculation uses it |
| #30 (configuration system) | OPEN | Configuration system IS implemented (`ShardsConfig::load_hierarchy()`); issue tracks further enhancements |

### Evidence Chain

**Current threshold is hardcoded but ready for config:**
```rust
// src/health/operations.rs:7
static IDLE_THRESHOLD_MINUTES: AtomicI64 = AtomicI64::new(10);

// Already has setter function - lines 10-12
pub fn set_idle_threshold_minutes(minutes: i64) {
    IDLE_THRESHOLD_MINUTES.store(minutes, Ordering::Relaxed);
}
```

**Configuration system already supports extension:**
```rust
// src/core/config.rs:54-64
pub struct ShardsConfig {
    pub agent: AgentConfig,
    pub terminal: TerminalConfig,
    pub agents: HashMap<String, AgentSettings>,
    pub include_patterns: Option<IncludeConfig>,
    // Can add: pub health: HealthConfig,
}
```

**CLI flag patterns established:**
```rust
// src/cli/app.rs:115-135 - current health command
.subcommand(
    Command::new("health")
        .arg(Arg::new("branch").index(1))
        .arg(Arg::new("all").long("all").action(ArgAction::SetTrue))
        .arg(Arg::new("json").long("json").action(ArgAction::SetTrue))
)
```

### Affected Files

| File | Lines | Action | Description |
|------|-------|--------|-------------|
| `src/cli/app.rs` | 115-135 | UPDATE | Add `--watch` and `--interval` flags to health subcommand |
| `src/cli/commands.rs` | 326-390 | UPDATE | Implement watch mode loop in `handle_health_command` |
| `src/core/config.rs` | 54-64 | UPDATE | Add `HealthConfig` struct and field to `ShardsConfig` |
| `src/health/types.rs` | 1-42 | UPDATE | Add `HealthHistoryEntry` struct for historical storage |
| `src/health/operations.rs` | 1-116 | UPDATE | Add threshold loading from config, history aggregation functions |
| `src/health/handler.rs` | 1-84 | UPDATE | Add `load_config_thresholds()` call on startup |
| `src/health/storage.rs` | NEW | CREATE | Historical metrics storage (JSON files initially) |
| `src/health/mod.rs` | 1-10 | UPDATE | Export new storage module |

### Integration Points

- `src/cli/commands.rs:326` - entry point calls `health::get_health_all_sessions()`
- `src/health/handler.rs:10` - calls `sessions::handler::list_sessions()`
- `src/health/operations.rs:7` - threshold constant used by `calculate_health_status()`
- `src/core/config.rs:119` - `load_hierarchy()` called in CLI commands

### Git History

- **Health module introduced**: 568ed29 - "feat: Add shards health command with process monitoring (#18)"
- **Last activity tracking**: bf8b380 - "Fix: Implement last_activity tracking for health monitoring (#26) (#40)"
- **Implication**: Foundation is solid; this enhancement extends existing working code

---

## Implementation Plan

### Phase 1: Configurable Thresholds

#### Step 1.1: Add HealthConfig to configuration schema

**File**: `src/core/config.rs`
**Lines**: 54-64
**Action**: UPDATE

**Current code:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShardsConfig {
    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default)]
    pub terminal: TerminalConfig,
    #[serde(default)]
    pub agents: HashMap<String, AgentSettings>,
    #[serde(default)]
    pub include_patterns: Option<IncludeConfig>,
}
```

**Required change:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShardsConfig {
    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default)]
    pub terminal: TerminalConfig,
    #[serde(default)]
    pub agents: HashMap<String, AgentSettings>,
    #[serde(default)]
    pub include_patterns: Option<IncludeConfig>,
    #[serde(default)]
    pub health: HealthConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    #[serde(default = "default_idle_threshold")]
    pub idle_threshold_minutes: i64,
    #[serde(default = "default_stuck_threshold")]
    pub stuck_threshold_minutes: i64,
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: u64,
    #[serde(default)]
    pub history_enabled: bool,
    #[serde(default = "default_history_retention")]
    pub history_retention_days: u64,
}

fn default_idle_threshold() -> i64 { 10 }
fn default_stuck_threshold() -> i64 { 30 }
fn default_refresh_interval() -> u64 { 5 }
fn default_history_retention() -> u64 { 7 }

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            idle_threshold_minutes: 10,
            stuck_threshold_minutes: 30,
            refresh_interval_secs: 5,
            history_enabled: false,
            history_retention_days: 7,
        }
    }
}
```

**Why**: Enables users to customize health thresholds via TOML config files.

---

#### Step 1.2: Update merge_configs for HealthConfig

**File**: `src/core/config.rs`
**Lines**: 189-213
**Action**: UPDATE

Add to the `merge_configs` function:
```rust
health: HealthConfig {
    idle_threshold_minutes: override_config.health.idle_threshold_minutes,
    stuck_threshold_minutes: override_config.health.stuck_threshold_minutes,
    refresh_interval_secs: override_config.health.refresh_interval_secs,
    history_enabled: override_config.health.history_enabled || base.health.history_enabled,
    history_retention_days: override_config.health.history_retention_days,
},
```

---

#### Step 1.3: Load thresholds in health handler

**File**: `src/health/handler.rs`
**Lines**: 7-10
**Action**: UPDATE

**Add at beginning of get_health_all_sessions:**
```rust
pub fn get_health_all_sessions() -> Result<HealthOutput, HealthError> {
    // Load config and apply thresholds
    if let Ok(config) = crate::core::config::ShardsConfig::load_hierarchy() {
        operations::set_idle_threshold_minutes(config.health.idle_threshold_minutes);
    }

    info!(event = "health.get_all_started");
    // ... rest of function
}
```

**Why**: Applies user-configured thresholds before calculating health status.

---

### Phase 2: Watch Mode

#### Step 2.1: Add CLI flags for watch mode

**File**: `src/cli/app.rs`
**Lines**: 115-135
**Action**: UPDATE

**Current code:**
```rust
.subcommand(
    Command::new("health")
        .about("Show health status and metrics for shards")
        .arg(Arg::new("branch").help("...").index(1))
        .arg(Arg::new("all").long("all").help("...").action(ArgAction::SetTrue))
        .arg(Arg::new("json").long("json").help("...").action(ArgAction::SetTrue))
)
```

**Required change:**
```rust
.subcommand(
    Command::new("health")
        .about("Show health status and metrics for shards")
        .arg(Arg::new("branch").help("Branch name of specific shard to check (optional)").index(1))
        .arg(Arg::new("all").long("all").help("Show health for all projects, not just current").action(ArgAction::SetTrue))
        .arg(Arg::new("json").long("json").help("Output in JSON format").action(ArgAction::SetTrue))
        .arg(
            Arg::new("watch")
                .long("watch")
                .short('w')
                .help("Continuously refresh health display")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("interval")
                .long("interval")
                .short('i')
                .help("Refresh interval in seconds (default: 5)")
                .value_parser(clap::value_parser!(u64))
                .default_value("5")
        )
)
```

**Why**: Enables `shards health --watch --interval 3` for continuous monitoring.

---

#### Step 2.2: Implement watch loop in handler

**File**: `src/cli/commands.rs`
**Lines**: 326-390
**Action**: UPDATE

**Add watch mode logic after reading flags:**
```rust
fn handle_health_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches.get_one::<String>("branch");
    let show_all = matches.get_flag("all");
    let json_output = matches.get_flag("json");
    let watch_mode = matches.get_flag("watch");
    let interval = matches.get_one::<u64>("interval").copied().unwrap_or(5);

    if watch_mode {
        run_health_watch_loop(branch, show_all, json_output, interval)?;
    } else {
        run_health_once(branch, show_all, json_output)?;
    }

    Ok(())
}

fn run_health_watch_loop(
    branch: Option<&String>,
    show_all: bool,
    json_output: bool,
    interval_secs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{self, Write};

    loop {
        // Clear screen (ANSI escape)
        print!("\x1B[2J\x1B[1;1H");
        io::stdout().flush()?;

        run_health_once(branch, show_all, json_output)?;

        println!("\nRefreshing every {}s. Press Ctrl+C to exit.", interval_secs);

        std::thread::sleep(std::time::Duration::from_secs(interval_secs));
    }
}

fn run_health_once(
    branch: Option<&String>,
    show_all: bool,
    json_output: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Move existing logic from handle_health_command here
    // ...
}
```

**Why**: Implements continuous monitoring with screen refresh for real-time health visibility.

---

### Phase 3: Historical Metrics (Optional - Lower Priority)

#### Step 3.1: Create health storage module

**File**: `src/health/storage.rs`
**Action**: CREATE

```rust
//! Historical health metrics storage
//!
//! Stores health snapshots over time for trend analysis.

use std::path::PathBuf;
use std::fs;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::health::types::{HealthOutput, HealthStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSnapshot {
    pub timestamp: DateTime<Utc>,
    pub total_shards: usize,
    pub working: usize,
    pub idle: usize,
    pub stuck: usize,
    pub crashed: usize,
    pub avg_cpu_percent: Option<f32>,
    pub total_memory_mb: Option<u64>,
}

impl From<&HealthOutput> for HealthSnapshot {
    fn from(output: &HealthOutput) -> Self {
        let (cpu_sum, cpu_count) = output.shards.iter()
            .filter_map(|s| s.metrics.cpu_usage_percent)
            .fold((0.0, 0), |(sum, count), cpu| (sum + cpu, count + 1));

        let total_mem: u64 = output.shards.iter()
            .filter_map(|s| s.metrics.memory_usage_mb)
            .sum();

        Self {
            timestamp: Utc::now(),
            total_shards: output.total_count,
            working: output.working_count,
            idle: output.idle_count,
            stuck: output.stuck_count,
            crashed: output.crashed_count,
            avg_cpu_percent: if cpu_count > 0 { Some(cpu_sum / cpu_count as f32) } else { None },
            total_memory_mb: if total_mem > 0 { Some(total_mem) } else { None },
        }
    }
}

pub fn get_history_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Could not find home directory")
        .join(".shards")
        .join("health_history")
}

pub fn save_snapshot(snapshot: &HealthSnapshot) -> Result<(), std::io::Error> {
    let history_dir = get_history_dir();
    fs::create_dir_all(&history_dir)?;

    let filename = format!("{}.json", snapshot.timestamp.format("%Y-%m-%d"));
    let filepath = history_dir.join(filename);

    // Append to daily file
    let mut snapshots: Vec<HealthSnapshot> = if filepath.exists() {
        let content = fs::read_to_string(&filepath)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    };

    snapshots.push(snapshot.clone());
    fs::write(&filepath, serde_json::to_string_pretty(&snapshots)?)?;

    Ok(())
}

pub fn load_history(days: u64) -> Result<Vec<HealthSnapshot>, std::io::Error> {
    let history_dir = get_history_dir();
    let mut all_snapshots = Vec::new();

    let cutoff = Utc::now() - chrono::Duration::days(days as i64);

    if let Ok(entries) = fs::read_dir(&history_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if let Ok(snapshots) = serde_json::from_str::<Vec<HealthSnapshot>>(&content) {
                    all_snapshots.extend(
                        snapshots.into_iter()
                            .filter(|s| s.timestamp > cutoff)
                    );
                }
            }
        }
    }

    all_snapshots.sort_by_key(|s| s.timestamp);
    Ok(all_snapshots)
}

pub fn cleanup_old_history(retention_days: u64) -> Result<usize, std::io::Error> {
    let history_dir = get_history_dir();
    let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);
    let cutoff_date = cutoff.format("%Y-%m-%d").to_string();

    let mut removed = 0;

    if let Ok(entries) = fs::read_dir(&history_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let filename = entry.file_name().to_string_lossy().to_string();
            if filename < cutoff_date && filename.ends_with(".json") {
                if fs::remove_file(entry.path()).is_ok() {
                    removed += 1;
                }
            }
        }
    }

    Ok(removed)
}
```

**Why**: Enables tracking health over time for trend analysis.

---

#### Step 3.2: Export storage module

**File**: `src/health/mod.rs`
**Lines**: 1-10
**Action**: UPDATE

```rust
pub mod errors;
pub mod handler;
pub mod operations;
pub mod storage;
pub mod types;

pub use errors::HealthError;
pub use handler::{get_health_all_sessions, get_health_single_session};
pub use storage::{HealthSnapshot, save_snapshot, load_history};
pub use types::{HealthMetrics, HealthOutput, HealthStatus, ShardHealth};
```

---

#### Step 3.3: Integrate history saving in watch mode

**File**: `src/cli/commands.rs` (in watch loop)
**Action**: UPDATE

```rust
fn run_health_watch_loop(...) -> Result<...> {
    let config = ShardsConfig::load_hierarchy().unwrap_or_default();

    loop {
        // ... existing logic ...

        if config.health.history_enabled {
            if let Ok(output) = health::get_health_all_sessions() {
                let snapshot = health::HealthSnapshot::from(&output);
                let _ = health::save_snapshot(&snapshot);
            }
        }

        std::thread::sleep(...);
    }
}
```

---

### Phase 4: Add Tests

#### Step 4.1: Add CLI flag tests

**File**: `src/cli/app.rs`
**Action**: UPDATE (add to existing tests module)

```rust
#[test]
fn test_cli_health_watch_mode() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec![
        "shards", "health", "--watch", "--interval", "10"
    ]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let health_matches = matches.subcommand_matches("health").unwrap();
    assert!(health_matches.get_flag("watch"));
    assert_eq!(*health_matches.get_one::<u64>("interval").unwrap(), 10);
}
```

---

#### Step 4.2: Add config tests

**File**: `src/core/config.rs`
**Action**: UPDATE (add to existing tests module)

```rust
#[test]
fn test_health_config_defaults() {
    let config = ShardsConfig::default();
    assert_eq!(config.health.idle_threshold_minutes, 10);
    assert_eq!(config.health.stuck_threshold_minutes, 30);
    assert_eq!(config.health.refresh_interval_secs, 5);
    assert!(!config.health.history_enabled);
}

#[test]
fn test_health_config_from_toml() {
    let config: ShardsConfig = toml::from_str(r#"
[health]
idle_threshold_minutes = 5
stuck_threshold_minutes = 15
history_enabled = true
"#).unwrap();
    assert_eq!(config.health.idle_threshold_minutes, 5);
    assert!(config.health.history_enabled);
}
```

---

## Patterns to Follow

**From codebase - CLI flag pattern:**

```rust
// SOURCE: src/cli/app.rs:87-113 (cleanup command)
.subcommand(
    Command::new("cleanup")
        .arg(
            Arg::new("older-than")
                .long("older-than")
                .help("Clean sessions older than N days (e.g., 7)")
                .value_name("DAYS")
                .value_parser(clap::value_parser!(u64))
        )
)
```

**From codebase - Configuration extension pattern:**

```rust
// SOURCE: src/core/config.rs:76-84
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalConfig {
    #[serde(default)]
    pub preferred: Option<String>,
    #[serde(default)]
    pub spawn_delay_ms: u64,
}

impl Default for TerminalConfig {
    fn default() -> Self { ... }
}
```

---

## Edge Cases & Risks

| Risk/Edge Case | Mitigation |
|----------------|------------|
| Watch mode in non-TTY (piped output) | Check `atty::is(atty::Stream::Stdout)` before clearing screen |
| Interval of 0 seconds | Validate interval >= 1 in CLI parser with `value_parser` |
| History files grow unbounded | Implement cleanup in watch loop based on `history_retention_days` |
| Config file parse errors | Already handled - `load_hierarchy()` uses `unwrap_or_default()` |
| Process exits during watch | Each iteration is independent; continues after process death |

---

## Validation

### Automated Checks

```bash
cargo check
cargo test
cargo clippy
```

### Manual Verification

1. `shards health --watch` - Verify continuous refresh with screen clear
2. `shards health --watch --interval 2` - Verify custom interval works
3. Create `~/.shards/config.toml` with `[health] idle_threshold_minutes = 2` - Verify threshold applies
4. Set `history_enabled = true` - Verify JSON files created in `~/.shards/health_history/`

---

## Scope Boundaries

**IN SCOPE:**
- Watch mode with `--watch` and `--interval` flags
- Configurable thresholds via `[health]` section in config.toml
- Historical metrics storage (JSON-based, opt-in)
- Basic history cleanup based on retention days

**OUT OF SCOPE (defer to future issues):**
- SQLite storage for metrics (JSON is sufficient initially)
- External metrics systems integration (Prometheus, etc.)
- Real-time PTY activity detection (complex; use file mtime heuristics if needed)
- Per-agent threshold configuration (can add later to HealthConfig)
- Memory/CPU trend visualization in CLI (use JSON export + external tools)
- Remote shard monitoring

---

## Suggested Implementation Order

1. **Phase 1 (Configurable Thresholds)** - Low risk, immediate value
2. **Phase 2 (Watch Mode)** - Medium complexity, high user value
3. **Phase 3 (Historical Metrics)** - Lower priority, can be deferred

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-21T12:35:00Z
- **Artifact**: `.archon/artifacts/issues/issue-31.md`
