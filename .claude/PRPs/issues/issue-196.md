# Investigation: kild create uses stale local HEAD instead of fetching latest base branch

**Issue**: #196 (https://github.com/Wirasm/kild/issues/196)
**Type**: BUG
**Investigated**: 2026-02-01T12:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                           |
| ---------- | ------ | ------------------------------------------------------------------------------------------------------------------- |
| Severity   | HIGH   | Every kild created while local main is behind remote starts from stale code, causing avoidable merge conflicts on PRs |
| Complexity | MEDIUM | 5 files modified but changes are well-scoped: config struct, defaults, git handler, session handler, CLI args         |
| Confidence | HIGH   | Root cause is clear at `git/handler.rs:178` - `repo.head()` used without fetching, fix path is well-understood        |

---

## Problem Statement

`kild create` branches new worktrees from `repo.head()` — the local HEAD commit — without fetching from remote first. If the user's local main branch is behind `origin/main`, new kilds start from stale code, causing merge conflicts when the PR is opened. This is particularly problematic for parallel development (the core kild use case) where main moves frequently.

---

## Analysis

### Root Cause

WHY: Kilds start from stale code, causing merge conflicts on PRs
- BECAUSE: The new branch is created from the local HEAD commit
- Evidence: `crates/kild-core/src/git/handler.rs:178-182`
```rust
let head = repo.head().map_err(git2_error)?;
let head_commit = head.peel_to_commit().map_err(git2_error)?;
repo.branch(&kild_branch, &head_commit, false)
    .map_err(git2_error)?;
```

- BECAUSE: No fetch operation occurs anywhere in the create flow
- Evidence: The entire chain CLI (`commands.rs:85`) -> `create_session()` (`sessions/handler.rs:44`) -> `create_worktree()` (`git/handler.rs:119`) never contacts the remote

- ROOT CAUSE: `create_worktree()` uses `repo.head()` as the base commit without first fetching from remote and without offering the option to base from a remote tracking branch
- Evidence: `crates/kild-core/src/git/handler.rs:119-262` - no fetch, no remote reference resolution

### Evidence Chain

WHY: Merge conflicts on kild PRs
- BECAUSE: Branch created from stale local HEAD
  Evidence: `git/handler.rs:178` - `repo.head()` returns local HEAD which may be behind remote
- BECAUSE: No fetch before branch creation
  Evidence: No `git fetch` or remote interaction in `create_worktree()` or `create_session()`
- ROOT CAUSE: Missing fetch + remote base resolution in create flow
  Evidence: `git/handler.rs:178-182` - hard-coded to use local HEAD

### Affected Files

| File                                        | Lines   | Action | Description                                             |
| ------------------------------------------- | ------- | ------ | ------------------------------------------------------- |
| `crates/kild-core/src/config/types.rs`      | 54-87   | UPDATE | Add `GitConfig` struct and field to `KildConfig`         |
| `crates/kild-core/src/config/defaults.rs`   | NEW     | UPDATE | Add default functions for git config                     |
| `crates/kild-core/src/config/loading.rs`    | 119-167 | UPDATE | Add git config merging to `merge_configs()`              |
| `crates/kild-core/src/git/handler.rs`       | 119-262 | UPDATE | Add fetch + remote base resolution in `create_worktree()` |
| `crates/kild-core/src/git/errors.rs`        | 4-43    | UPDATE | Add `FetchFailed` error variant                          |
| `crates/kild/src/app.rs`                    | 18-58   | UPDATE | Add `--base` and `--no-fetch` CLI flags to create command |
| `crates/kild/src/commands.rs`               | 85-150  | UPDATE | Handle `--base` and `--no-fetch` overrides               |
| `crates/kild-core/src/sessions/types.rs`    | 269-320 | UPDATE | Add `base_branch` and `no_fetch` to `CreateSessionRequest` |

### Integration Points

- `crates/kild/src/commands.rs:117` calls `session_handler::create_session(request, &config)`
- `crates/kild-core/src/sessions/handler.rs:130-136` calls `git::handler::create_worktree()`
- `crates/kild-ui/src/actions.rs:54-60` also creates `CreateSessionRequest` (UI path)
- `crates/kild-core/src/config/loading.rs:119` `merge_configs()` must merge new git section

### Git History

- **Introduced**: Original implementation - `create_worktree()` never had fetch logic
- **Last modified**: `9e488d1` - branch prefix rename (kild_ to kild/)
- **Implication**: Long-standing design gap, not a regression

---

## Implementation Plan

### Step 1: Add `GitConfig` to config types

**File**: `crates/kild-core/src/config/types.rs`
**Lines**: 26-92
**Action**: UPDATE

**Current code (lines 54-75):**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KildConfig {
    #[serde(default)]
    pub agent: AgentConfig,

    #[serde(default)]
    pub terminal: TerminalConfig,

    #[serde(default)]
    pub agents: HashMap<String, AgentSettings>,

    #[serde(default = "default_include_patterns_option")]
    pub include_patterns: Option<IncludeConfig>,

    #[serde(default)]
    pub health: HealthConfig,
}
```

**Required change:**

Add `GitConfig` struct and a `git` field to `KildConfig`:

```rust
/// Git configuration for worktree creation.
///
/// Controls how new worktrees are branched - which remote to fetch from
/// and which branch to use as the base for new kild branches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitConfig {
    /// Remote name to fetch from before creating worktrees.
    /// Default: "origin"
    #[serde(default = "super::defaults::default_git_remote")]
    pub remote: String,

    /// Base branch to fetch and create new worktrees from.
    /// Default: "main"
    #[serde(default = "super::defaults::default_git_base_branch")]
    pub base_branch: String,

    /// Whether to fetch the base branch from remote before creating a worktree.
    /// Default: true
    #[serde(default = "super::defaults::default_fetch_before_create")]
    pub fetch_before_create: bool,
}
```

Add `git` field to `KildConfig`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KildConfig {
    #[serde(default)]
    pub agent: AgentConfig,

    #[serde(default)]
    pub terminal: TerminalConfig,

    #[serde(default)]
    pub agents: HashMap<String, AgentSettings>,

    #[serde(default = "default_include_patterns_option")]
    pub include_patterns: Option<IncludeConfig>,

    #[serde(default)]
    pub health: HealthConfig,

    /// Git configuration for worktree creation
    #[serde(default)]
    pub git: GitConfig,
}
```

Update `Default for KildConfig` to include the new field:

```rust
impl Default for KildConfig {
    fn default() -> Self {
        Self {
            agent: AgentConfig::default(),
            terminal: TerminalConfig::default(),
            agents: HashMap::default(),
            include_patterns: default_include_patterns_option(),
            health: HealthConfig::default(),
            git: GitConfig::default(),
        }
    }
}
```

**Why**: Need a config structure to hold git-related settings that flow through the config hierarchy.

---

### Step 2: Add default functions for git config

**File**: `crates/kild-core/src/config/defaults.rs`
**Action**: UPDATE

**Required change:**

Add default functions and `Default` impl for `GitConfig`:

```rust
use crate::config::types::GitConfig;  // add to existing imports

/// Returns the default git remote name ("origin").
pub fn default_git_remote() -> String {
    "origin".to_string()
}

/// Returns the default base branch ("main").
pub fn default_git_base_branch() -> String {
    "main".to_string()
}

/// Returns whether to fetch before creating worktrees (true).
pub fn default_fetch_before_create() -> bool {
    true
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            remote: default_git_remote(),
            base_branch: default_git_base_branch(),
            fetch_before_create: default_fetch_before_create(),
        }
    }
}
```

**Why**: Serde needs default functions for deserialization when fields are missing from TOML.

---

### Step 3: Add git config merging to `merge_configs()`

**File**: `crates/kild-core/src/config/loading.rs`
**Lines**: 115-167
**Action**: UPDATE

**Current code (lines 119-167):**
```rust
pub fn merge_configs(base: KildConfig, override_config: KildConfig) -> KildConfig {
    KildConfig {
        agent: AgentConfig { ... },
        terminal: TerminalConfig { ... },
        agents: { ... },
        include_patterns: merge_include_patterns(...),
        health: HealthConfig { ... },
    }
}
```

**Required change:**

Add `git` field to the `merge_configs` return value:

```rust
git: GitConfig {
    remote: override_config.git.remote,
    base_branch: override_config.git.base_branch,
    fetch_before_create: override_config.git.fetch_before_create,
},
```

**Why**: Git config follows the same override pattern as terminal config - override always wins (non-optional fields). This matches the existing pattern where project config overrides user config.

---

### Step 4: Add `FetchFailed` error variant

**File**: `crates/kild-core/src/git/errors.rs`
**Lines**: 4-43
**Action**: UPDATE

**Required change:**

Add a new error variant to `GitError`:

```rust
#[error("Failed to fetch from remote '{remote}': {message}")]
FetchFailed { remote: String, message: String },
```

Add to `KildError` impl:

```rust
GitError::FetchFailed { .. } => "GIT_FETCH_FAILED",
```

`is_user_error()` should return `false` for `FetchFailed` (it's a network/auth issue, not user input error).

**Why**: Need a specific error type for fetch failures to surface clearly to the user.

---

### Step 5: Add fetch + remote base resolution to `create_worktree()`

**File**: `crates/kild-core/src/git/handler.rs`
**Lines**: 119-262
**Action**: UPDATE

**Current code (lines 170-189):**
```rust
if !branch_exists {
    debug!(
        event = "core.git.branch.create_started",
        project_id = project.id,
        branch = kild_branch
    );

    let head = repo.head().map_err(git2_error)?;
    let head_commit = head.peel_to_commit().map_err(git2_error)?;

    repo.branch(&kild_branch, &head_commit, false)
        .map_err(git2_error)?;

    debug!(
        event = "core.git.branch.create_completed",
        project_id = project.id,
        branch = kild_branch
    );
}
```

**Required change:**

Update `create_worktree` signature to accept git config:

```rust
pub fn create_worktree(
    base_dir: &Path,
    project: &ProjectInfo,
    branch: &str,
    config: Option<&KildConfig>,
    git_config: &GitConfig,
) -> Result<WorktreeInfo, GitError> {
```

Add fetch before branch creation and resolve remote base:

```rust
if !branch_exists {
    debug!(
        event = "core.git.branch.create_started",
        project_id = project.id,
        branch = kild_branch
    );

    // Fetch latest base branch from remote if configured
    if git_config.fetch_before_create {
        fetch_remote(&project.path, &git_config.remote, &git_config.base_branch)?;
    }

    // Resolve base commit: prefer remote tracking branch, fall back to HEAD
    let base_commit = resolve_base_commit(&repo, git_config)?;

    repo.branch(&kild_branch, &base_commit, false)
        .map_err(git2_error)?;

    debug!(
        event = "core.git.branch.create_completed",
        project_id = project.id,
        branch = kild_branch
    );
}
```

Add helper functions:

```rust
/// Fetch a specific branch from a remote using git CLI.
///
/// Uses `git fetch` CLI to inherit the user's existing auth setup
/// (SSH agent, credential helpers, etc.) with zero auth code.
fn fetch_remote(repo_path: &Path, remote: &str, branch: &str) -> Result<(), GitError> {
    info!(
        event = "core.git.fetch_started",
        remote = remote,
        branch = branch,
        repo_path = %repo_path.display()
    );

    let output = std::process::Command::new("git")
        .current_dir(repo_path)
        .args(["fetch", remote, branch])
        .output()
        .map_err(|e| GitError::FetchFailed {
            remote: remote.to_string(),
            message: format!("Failed to execute git: {}", e),
        })?;

    if output.status.success() {
        info!(
            event = "core.git.fetch_completed",
            remote = remote,
            branch = branch
        );
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!(
            event = "core.git.fetch_failed",
            remote = remote,
            branch = branch,
            stderr = %stderr.trim()
        );
        Err(GitError::FetchFailed {
            remote: remote.to_string(),
            message: stderr.trim().to_string(),
        })
    }
}

/// Resolve the base commit for a new branch.
///
/// Tries the remote tracking branch first (e.g., `origin/main`),
/// falls back to local HEAD if the remote ref doesn't exist.
fn resolve_base_commit<'repo>(
    repo: &'repo Repository,
    git_config: &GitConfig,
) -> Result<git2::Commit<'repo>, GitError> {
    let remote_ref = format!("refs/remotes/{}/{}", git_config.remote, git_config.base_branch);

    match repo.find_reference(&remote_ref) {
        Ok(reference) => {
            let commit = reference.peel_to_commit().map_err(git2_error)?;
            info!(
                event = "core.git.base_resolved",
                source = "remote",
                reference = remote_ref,
                commit = %commit.id()
            );
            Ok(commit)
        }
        Err(_) => {
            // Remote ref not found - fall back to HEAD
            warn!(
                event = "core.git.base_fallback_to_head",
                remote_ref = remote_ref,
                reason = "remote tracking branch not found"
            );
            let head = repo.head().map_err(git2_error)?;
            let commit = head.peel_to_commit().map_err(git2_error)?;
            info!(
                event = "core.git.base_resolved",
                source = "head",
                commit = %commit.id()
            );
            Ok(commit)
        }
    }
}
```

**Why**: This is the core fix. Fetches latest remote state before branching and uses the remote tracking branch as the base instead of local HEAD. Falls back to HEAD gracefully if the remote ref doesn't exist (e.g., first time, no remote set up).

---

### Step 6: Update `create_session()` to pass git config

**File**: `crates/kild-core/src/sessions/handler.rs`
**Lines**: 130-136
**Action**: UPDATE

**Current code:**
```rust
let worktree = git::handler::create_worktree(
    &base_config.kild_dir,
    &project,
    &validated.name,
    Some(kild_config),
)
.map_err(|e| SessionError::GitError { source: e })?;
```

**Required change:**

Apply CLI overrides to git config and pass it:

```rust
// Build effective git config with CLI overrides
let mut git_config = kild_config.git.clone();
if let Some(base) = &request.base_branch {
    git_config.base_branch = base.clone();
}
if request.no_fetch {
    git_config.fetch_before_create = false;
}

let worktree = git::handler::create_worktree(
    &base_config.kild_dir,
    &project,
    &validated.name,
    Some(kild_config),
    &git_config,
)
.map_err(|e| SessionError::GitError { source: e })?;
```

**Why**: CLI overrides (`--base`, `--no-fetch`) need to take effect without modifying the loaded config.

---

### Step 7: Add `base_branch` and `no_fetch` to `CreateSessionRequest`

**File**: `crates/kild-core/src/sessions/types.rs`
**Lines**: 269-320
**Action**: UPDATE

**Current code:**
```rust
#[derive(Debug, Clone)]
pub struct CreateSessionRequest {
    pub branch: String,
    pub agent: Option<String>,
    pub note: Option<String>,
    pub project_path: Option<PathBuf>,
}
```

**Required change:**

```rust
#[derive(Debug, Clone)]
pub struct CreateSessionRequest {
    pub branch: String,
    pub agent: Option<String>,
    pub note: Option<String>,
    pub project_path: Option<PathBuf>,
    /// Override base branch for this create (CLI --base flag)
    pub base_branch: Option<String>,
    /// Skip fetching before create (CLI --no-fetch flag)
    pub no_fetch: bool,
}
```

Update `new()` and `with_project_path()` to initialize new fields:

```rust
impl CreateSessionRequest {
    pub fn new(branch: String, agent: Option<String>, note: Option<String>) -> Self {
        Self {
            branch,
            agent,
            note,
            project_path: None,
            base_branch: None,
            no_fetch: false,
        }
    }

    pub fn with_project_path(
        branch: String,
        agent: Option<String>,
        note: Option<String>,
        project_path: PathBuf,
    ) -> Self {
        Self {
            branch,
            agent,
            note,
            project_path: Some(project_path),
            base_branch: None,
            no_fetch: false,
        }
    }
}
```

**Why**: CLI overrides need to flow from CLI layer through to session handler.

---

### Step 8: Add `--base` and `--no-fetch` CLI flags

**File**: `crates/kild/src/app.rs`
**Lines**: 18-58
**Action**: UPDATE

**Current code (create subcommand, lines 18-58):**
```rust
.subcommand(
    Command::new("create")
        .about("Create a new kild with git worktree and launch agent")
        .arg(Arg::new("branch")...)
        .arg(Arg::new("agent")...)
        .arg(Arg::new("terminal")...)
        .arg(Arg::new("startup-command")...)
        .arg(Arg::new("flags")...)
        .arg(Arg::new("note")...)
)
```

**Required change:**

Add two new args after `note`:

```rust
.arg(
    Arg::new("base")
        .long("base")
        .short('b')
        .help("Base branch to create worktree from (overrides config, default: main)")
)
.arg(
    Arg::new("no-fetch")
        .long("no-fetch")
        .help("Skip fetching from remote before creating worktree")
        .action(ArgAction::SetTrue)
)
```

**Why**: Users need CLI overrides for base branch (e.g., `--base develop`) and to skip fetch for offline use (`--no-fetch`).

---

### Step 9: Handle new CLI flags in `handle_create_command()`

**File**: `crates/kild/src/commands.rs`
**Lines**: 85-150
**Action**: UPDATE

**Current code (lines 85-115):**
```rust
fn handle_create_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches.get_one::<String>("branch").ok_or("...")?;
    let note = matches.get_one::<String>("note").cloned();
    let mut config = load_config_with_warning();
    // ... existing CLI overrides ...
    let request = CreateSessionRequest::new(branch.clone(), agent_override, note);
    // ...
}
```

**Required change:**

Extract new flags and set them on the request:

```rust
let base_branch = matches.get_one::<String>("base").cloned();
let no_fetch = matches.get_flag("no-fetch");

let mut request = CreateSessionRequest::new(branch.clone(), agent_override, note);
request.base_branch = base_branch;
request.no_fetch = no_fetch;
```

**Why**: Wire CLI flags through to the request object.

---

### Step 10: Add tests

**File**: `crates/kild-core/src/config/types.rs` (test module)
**Action**: UPDATE

Add serialization test for `GitConfig`:

```rust
#[test]
fn test_git_config_serialization() {
    let config = GitConfig::default();
    assert_eq!(config.remote, "origin");
    assert_eq!(config.base_branch, "main");
    assert!(config.fetch_before_create);

    let toml_str = toml::to_string(&config).unwrap();
    let parsed: GitConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.remote, config.remote);
    assert_eq!(parsed.base_branch, config.base_branch);
    assert_eq!(parsed.fetch_before_create, config.fetch_before_create);
}

#[test]
fn test_git_config_from_toml() {
    let config: KildConfig = toml::from_str(r#"
[git]
remote = "upstream"
base_branch = "develop"
fetch_before_create = false
"#).unwrap();
    assert_eq!(config.git.remote, "upstream");
    assert_eq!(config.git.base_branch, "develop");
    assert!(!config.git.fetch_before_create);
}

#[test]
fn test_git_config_defaults_when_missing() {
    let config: KildConfig = toml::from_str("").unwrap();
    assert_eq!(config.git.remote, "origin");
    assert_eq!(config.git.base_branch, "main");
    assert!(config.git.fetch_before_create);
}
```

**File**: `crates/kild-core/src/config/loading.rs` (test module)
**Action**: UPDATE

Add merge test for git config:

```rust
#[test]
fn test_git_config_merge() {
    let user_config: KildConfig = toml::from_str(r#"
[git]
remote = "upstream"
base_branch = "develop"
"#).unwrap();

    let project_config: KildConfig = toml::from_str(r#"
[git]
base_branch = "main"
"#).unwrap();

    let merged = merge_configs(user_config, project_config);
    // Project overrides base_branch
    assert_eq!(merged.git.base_branch, "main");
    // Project's default remote ("origin") overrides user's "upstream"
    // (same limitation as terminal config - non-optional fields always take override)
    assert_eq!(merged.git.remote, "origin");
}
```

**File**: `crates/kild/src/app.rs` (test module)
**Action**: UPDATE

Add CLI flag tests:

```rust
#[test]
fn test_cli_create_with_base_branch() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec![
        "kild", "create", "feature-auth", "--base", "develop"
    ]);
    assert!(matches.is_ok());
    let matches = matches.unwrap();
    let create_matches = matches.subcommand_matches("create").unwrap();
    assert_eq!(create_matches.get_one::<String>("base").unwrap(), "develop");
}

#[test]
fn test_cli_create_with_no_fetch() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec![
        "kild", "create", "feature-auth", "--no-fetch"
    ]);
    assert!(matches.is_ok());
    let matches = matches.unwrap();
    let create_matches = matches.subcommand_matches("create").unwrap();
    assert!(create_matches.get_flag("no-fetch"));
}

#[test]
fn test_cli_create_with_base_and_no_fetch() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec![
        "kild", "create", "feature-auth", "--base", "develop", "--no-fetch"
    ]);
    assert!(matches.is_ok());
}
```

**File**: `crates/kild-core/src/git/handler.rs` (test module)
**Action**: UPDATE

Add test for `resolve_base_commit` fallback:

```rust
#[test]
fn test_resolve_base_commit_falls_back_to_head() {
    let temp_dir = create_temp_test_dir("kild_test_resolve_base");
    init_test_repo(&temp_dir);

    let repo = Repository::open(&temp_dir).unwrap();
    let git_config = GitConfig {
        remote: "origin".to_string(),
        base_branch: "main".to_string(),
        fetch_before_create: false,
    };

    // No remote set up, should fall back to HEAD
    let commit = resolve_base_commit(&repo, &git_config).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(commit.id(), head.id());

    let _ = std::fs::remove_dir_all(&temp_dir);
}
```

---

## Patterns to Follow

**From codebase - git CLI shell-out pattern:**
```rust
// SOURCE: crates/kild-core/src/sessions/handler.rs:558-561
// Pattern for shelling out to git CLI with error handling
let output = std::process::Command::new("git")
    .current_dir(worktree_path)
    .args(["push", "origin", "--delete", branch])
    .output()
    .map_err(|e| SessionError::RemoteBranchDeleteFailed {
        branch: branch.to_string(),
        message: format!("Failed to execute git in {}: {}", worktree_path.display(), e),
    })?;
```

**From codebase - config section pattern:**
```rust
// SOURCE: crates/kild-core/src/config/types.rs:94-117
// Pattern for config struct with serde defaults
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HealthConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_threshold_minutes: Option<u64>,
    // ...
}
```

**From codebase - error variant pattern:**
```rust
// SOURCE: crates/kild-core/src/git/errors.rs:8-9
// Pattern for error variants with structured fields
#[error("Repository not found at path: {path}")]
RepositoryNotFound { path: String },
```

**From codebase - CLI flag override pattern:**
```rust
// SOURCE: crates/kild/src/commands.rs:94-106
// Pattern for applying CLI overrides to config
if let Some(agent) = &agent_override {
    config.agent.default = agent.clone();
}
```

---

## Edge Cases & Risks

| Risk/Edge Case                               | Mitigation                                                                     |
| -------------------------------------------- | ------------------------------------------------------------------------------ |
| No remote configured (fresh local repo)      | `fetch_remote()` returns `FetchFailed`, surfaced to user with clear message     |
| Offline / no network                          | User can use `--no-fetch` flag or set `fetch_before_create = false` in config   |
| Remote branch doesn't exist                  | `resolve_base_commit()` falls back to HEAD with a warning log                   |
| Auth failure on fetch                         | Git CLI inherits user's credential helpers; error surfaced via `FetchFailed`    |
| `--base` specifies non-existent branch       | Fetch will succeed but `resolve_base_commit` won't find the ref, falls back to HEAD with warning |
| Existing branch (recreating destroyed kild)  | `branch_exists` check at line 162 skips the entire fetch+branch block           |

---

## Validation

### Automated Checks

```bash
cargo fmt --check
cargo clippy --all -- -D warnings
cargo test --all
cargo build --all
```

### Manual Verification

1. Create a kild in a repo where local main is behind origin/main - verify it branches from origin/main
2. Create a kild with `--no-fetch` - verify it uses local HEAD
3. Create a kild with `--base develop` - verify it fetches and uses origin/develop
4. Create a kild in a repo with no remote - verify graceful error message
5. Verify config TOML parsing with `[git]` section works
6. Verify empty config still works (defaults applied)

---

## Scope Boundaries

**IN SCOPE:**
- Add `GitConfig` to config types with `remote`, `base_branch`, `fetch_before_create`
- Add `git fetch` via CLI before branch creation in `create_worktree()`
- Resolve remote tracking branch as base commit (with HEAD fallback)
- Add `--base` and `--no-fetch` CLI flags
- Add `base_branch` and `no_fetch` to `CreateSessionRequest`
- Config merging for git section
- Tests for new config, CLI flags, and base commit resolution

**OUT OF SCOPE (do not touch):**
- Auto-detecting default branch from `refs/remotes/origin/HEAD` (future improvement)
- Changing existing git2 usage patterns elsewhere
- UI changes for kild-ui (UI uses `CreateSessionRequest` which gets the new fields with defaults)
- Modifying existing tests unrelated to this change
- Adding git fetch to any other flow (destroy, complete, etc.)

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-01T12:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-196.md`
