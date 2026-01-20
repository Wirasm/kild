# Investigation: Implement last_activity tracking for health monitoring

**Issue**: #26 (https://github.com/Wirasm/shards/issues/26)
**Type**: ENHANCEMENT
**Investigated**: 2026-01-20T15:14:31.487+02:00

### Assessment

| Metric | Value | Reasoning |
|--------|-------|-----------|
| Priority | HIGH | Health monitoring feature is broken without this field - all sessions show as "Crashed" instead of proper status |
| Complexity | MEDIUM | Requires changes to 3-4 files (Session struct, health operations, session creation) with moderate integration points |
| Confidence | HIGH | Clear root cause identified with specific TODOs in code, well-understood implementation path |

---

## Problem Statement

The health monitoring system cannot distinguish between Idle and Stuck session states because the `last_activity` field is missing from the Session struct, causing all sessions to show as "Crashed" instead of proper health status.

---

## Analysis

### Root Cause / Change Rationale

The health monitoring feature was added in commit 568ed29 but the Session struct was never updated to include the `last_activity` field that the health operations expect. This creates a gap where health status calculation always receives `None` for activity data.

### Evidence Chain

WHY: Health command shows all sessions as "Crashed"
↓ BECAUSE: `calculate_health_status` receives `None` for `last_activity`
  Evidence: `src/health/operations.rs:58` - `None, // TODO: Implement last_activity tracking`

↓ BECAUSE: Session struct doesn't have `last_activity` field
  Evidence: `src/sessions/types.rs:10-45` - Session struct missing the field

↓ ROOT CAUSE: Field was never added when health monitoring was implemented
  Evidence: `src/health/operations.rs:74` - `last_activity: None, // TODO: Implement last_activity tracking`

### Affected Files

| File | Lines | Action | Description |
|------|-------|--------|-------------|
| `src/sessions/types.rs` | 45 | UPDATE | Add `last_activity` field to Session struct |
| `src/sessions/handler.rs` | 97, 408 | UPDATE | Set initial activity timestamp on session creation |
| `src/health/operations.rs` | 58, 74 | UPDATE | Use real activity data instead of None |
| `src/sessions/types.rs` | 120+ | UPDATE | Add serde default function for backward compatibility |

### Integration Points

- `src/health/operations.rs:58` calls `calculate_health_status` expecting activity data
- `src/sessions/handler.rs:97` creates sessions with `created_at` timestamp pattern
- Session persistence in `operations::save_session_to_file` needs backward compatibility
- Health command in CLI depends on proper status calculation

### Git History

- **Introduced**: 568ed29 - 2024 - "feat: Add shards health command with process monitoring (#18)"
- **Last modified**: 7dcd6e9 - recent - "fix: resolve compilation errors and update skill.md"
- **Implication**: Feature gap - health monitoring added but Session struct never updated

---

## Implementation Plan

### Step 1: Add last_activity field to Session struct

**File**: `src/sessions/types.rs`
**Lines**: 45
**Action**: UPDATE

**Current code:**
```rust
// Line 45 (after command field)
#[serde(default = "default_command")]
pub command: String,
```

**Required change:**
```rust
#[serde(default = "default_command")]
pub command: String,

/// Timestamp of last detected activity for health monitoring.
/// 
/// This tracks when the session was last active for health status calculation.
/// Initially set to session creation time, updated by activity monitoring.
/// 
/// Format: RFC3339 timestamp string (e.g., "2024-01-01T12:00:00Z")
#[serde(default)]
pub last_activity: Option<String>,
```

**Why**: Adds the missing field that health operations expect, with serde default for backward compatibility

---

### Step 2: Add serde default function

**File**: `src/sessions/types.rs`
**Lines**: 8
**Action**: UPDATE

**Current code:**
```rust
fn default_command() -> String { String::default() }
```

**Required change:**
```rust
fn default_command() -> String { String::default() }
fn default_last_activity() -> Option<String> { None }
```

**Why**: Provides default value for deserialization of existing session files

---

### Step 3: Initialize activity timestamp on session creation

**File**: `src/sessions/handler.rs`
**Lines**: 97
**Action**: UPDATE

**Current code:**
```rust
// Line 97
created_at: chrono::Utc::now().to_rfc3339(),
```

**Required change:**
```rust
created_at: chrono::Utc::now().to_rfc3339(),
last_activity: Some(chrono::Utc::now().to_rfc3339()),
```

**Why**: Sets initial activity timestamp to session creation time

---

### Step 4: Initialize activity timestamp on session restart

**File**: `src/sessions/handler.rs`
**Lines**: 408
**Action**: UPDATE

**Current code:**
```rust
// Around line 408 in restart_session
session.status = SessionStatus::Active;
```

**Required change:**
```rust
session.status = SessionStatus::Active;
session.last_activity = Some(chrono::Utc::now().to_rfc3339());
```

**Why**: Updates activity timestamp when session is restarted

---

### Step 5: Use real activity data in health operations

**File**: `src/health/operations.rs`
**Lines**: 58
**Action**: UPDATE

**Current code:**
```rust
// Line 58
None, // TODO: Implement last_activity tracking
```

**Required change:**
```rust
session.last_activity.as_deref(),
```

**Why**: Passes actual activity data instead of None

---

### Step 6: Use real activity data in health metrics

**File**: `src/health/operations.rs`
**Lines**: 74
**Action**: UPDATE

**Current code:**
```rust
// Line 74
last_activity: None, // TODO: Implement last_activity tracking
```

**Required change:**
```rust
last_activity: session.last_activity.clone(),
```

**Why**: Includes actual activity timestamp in health metrics output

---

### Step 7: Update test to include new field

**File**: `src/sessions/types.rs`
**Lines**: 120-140
**Action**: UPDATE

**Current code:**
```rust
// In test_session_creation
let session = Session {
    // ... existing fields ...
    command: "claude-code".to_string(),
};
```

**Required change:**
```rust
let session = Session {
    // ... existing fields ...
    command: "claude-code".to_string(),
    last_activity: Some("2024-01-01T00:00:00Z".to_string()),
};
```

**Why**: Ensures test passes with new required field

---

## Patterns to Follow

**From codebase - mirror these exactly:**

```rust
// SOURCE: src/sessions/handler.rs:97
// Pattern for timestamp creation
created_at: chrono::Utc::now().to_rfc3339(),
```

```rust
// SOURCE: src/sessions/types.rs:8
// Pattern for serde defaults
fn default_command() -> String { String::default() }
#[serde(default = "default_command")]
pub command: String,
```

```rust
// SOURCE: src/sessions/types.rs:25-35
// Pattern for optional fields with documentation
/// Process ID of the spawned terminal/agent process.
///
/// This is `None` if:
/// - The session was created before PID tracking was implemented
pub process_id: Option<u32>,
```

---

## Edge Cases & Risks

| Risk/Edge Case | Mitigation |
|----------------|------------|
| Existing session files fail to load | Use `#[serde(default)]` for backward compatibility |
| Activity timestamp format inconsistency | Use same RFC3339 format as `created_at` field |
| Performance impact of timestamp updates | Initial implementation only sets on create/restart |

---

## Validation

### Automated Checks

```bash
cargo check
cargo test sessions::types::tests
cargo test health::operations::tests
```

### Manual Verification

1. Create a new session and verify `last_activity` is set
2. Run `shards health` and verify sessions show proper status (not all "Crashed")
3. Load existing session files and verify they still work

---

## Scope Boundaries

**IN SCOPE:**
- Adding `last_activity` field to Session struct
- Setting initial timestamp on session create/restart
- Using real activity data in health operations
- Backward compatibility for existing session files

**OUT OF SCOPE (do not touch):**
- PTY integration for real-time activity tracking
- File monitoring for activity detection
- Advanced activity tracking mechanisms
- Health status calculation logic changes

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-20T15:14:31.487+02:00
- **Artifact**: `.archon/artifacts/issues/issue-26.md`
