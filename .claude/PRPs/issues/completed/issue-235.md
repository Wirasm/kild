# Investigation: Flaky CI - test_get_monitor_not_found panics on NSScreen.localizedName NULL

**Issue**: #235 (https://github.com/Wirasm/kild/issues/235)
**Type**: BUG
**Investigated**: 2026-02-05T12:00:00Z

### Assessment

| Metric | Value | Reasoning |
|--------|-------|-----------|
| Severity | MEDIUM | Single test failure in CI; all other 243 tests pass. No user-facing impact but blocks clean CI runs. |
| Complexity | LOW | Fix requires changes to 1 file (handler.rs); the panic originates in an external crate (xcap) and must be caught at the call site. |
| Confidence | HIGH | Clear panic trace from CI, confirmed by reading xcap source — `NSScreen.localizedName` returns NULL on headless runners and objc2-app-kit panics instead of returning an error. |

---

## Problem Statement

The test `test_get_monitor_not_found` in `kild-peek-core` intermittently panics on GitHub Actions macOS runners because `xcap`'s monitor name retrieval calls `NSScreen.localizedName`, which returns NULL on headless environments. The `objc2-app-kit` crate panics on NULL instead of returning an error, and this panic occurs *inside* xcap before kild-peek-core's existing `unwrap_or_else` fallback on line 213 can catch it.

---

## Analysis

### Root Cause

WHY 1: Why does `test_get_monitor_not_found` panic?
→ Because `get_monitor(999)` calls `list_monitors()` which enumerates all monitors, and during enumeration, calling `m.name()` panics.
→ Evidence: `crates/kild-peek-core/src/window/handler.rs:838` — `let monitors = list_monitors()?;`

WHY 2: Why does `m.name()` panic instead of returning an error?
→ Because xcap's `impl_monitor.name()` calls `get_display_friendly_name()` which internally calls `screen.localizedName().to_string()` in an unsafe block. The `objc2-app-kit` binding for `localizedName()` panics when the Objective-C method returns NULL, rather than returning an Option or error. The `unwrap_or` fallback on line 135 of xcap never executes because the panic happens before the function returns.
→ Evidence: `~/.cargo/registry/src/.../xcap-0.8.1/src/macos/impl_monitor.rs:40` — `unsafe { return Ok(screen.localizedName().to_string()) }`
→ Evidence: `~/.cargo/registry/src/.../xcap-0.8.1/src/macos/impl_monitor.rs:133-138` — fallback `unwrap_or` never reached

WHY 3: Why does `localizedName` return NULL?
→ Because GitHub Actions macOS runners are headless environments. They have virtual display devices (`CGGetActiveDisplayList` returns displays) but `NSScreen.localizedName` returns NULL since there's no actual physical display attached.
→ Evidence: CI run https://github.com/Wirasm/kild/actions/runs/21705677014/job/62596066422

ROOT CAUSE: xcap 0.8.1 panics (via objc2-app-kit) when `NSScreen.localizedName` returns NULL on headless macOS. This is a bug in xcap, but since it's an external dependency, kild-peek-core must protect itself by catching the panic at the `m.name()` call site.

### Evidence Chain

```
test_get_monitor_not_found (handler.rs:918)
  → get_monitor(999) (handler.rs:835)
    → list_monitors() (handler.rs:838)
      → xcap::Monitor::all() returns Ok (displays exist on CI)
      → iterate monitors, call m.name() (handler.rs:213)
        → xcap impl_monitor.name() (impl_monitor.rs:133)
          → get_display_friendly_name() (impl_monitor.rs:26)
            → screen.localizedName() (impl_monitor.rs:40)
              → objc2-app-kit PANICS: "unexpected NULL returned from -[NSScreen localizedName]"
```

### Affected Files

| File | Lines | Action | Description |
|------|-------|--------|-------------|
| `crates/kild-peek-core/src/window/handler.rs` | 213 | UPDATE | Wrap `m.name()` call with `std::panic::catch_unwind` to catch xcap/objc2 panics |
| `crates/kild-peek-core/src/window/handler.rs` | 893-896 | UPDATE | Add comment explaining the test may observe panics in CI |

### Integration Points

- `get_monitor()` at `handler.rs:838` calls `list_monitors()`
- `get_primary_monitor()` at `handler.rs:857` calls `list_monitors()`
- `screenshot/handler.rs` uses monitor enumeration for capture operations
- CLI `handle_list_monitors()` in `kild-peek/src/commands.rs:128` calls `list_monitors()`

### Git History

- **Introduced**: `0514fc4` - 2026-01-28 - "Add kild-peek CLI for native application inspection (#122)"
- **Last modified**: `06e4063` - "feat(kild-peek): add --wait/--timeout to interaction and element commands (#181)"
- **Implication**: Original bug — monitor code has always been vulnerable to this panic on headless environments. The issue became visible when CI started running these tests.

---

## Implementation Plan

### Step 1: Wrap `m.name()` with `catch_unwind` to handle xcap/objc2 panics

**File**: `crates/kild-peek-core/src/window/handler.rs`
**Lines**: 213
**Action**: UPDATE

**Current code:**
```rust
// Line 213
let name = m.name().unwrap_or_else(|_| format!("Monitor {}", idx));
```

**Required change:**
```rust
// Use catch_unwind to protect against xcap/objc2-app-kit panics
// when NSScreen.localizedName returns NULL on headless macOS (e.g. CI)
let name = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| m.name()))
    .unwrap_or_else(|_| {
        debug!(
            event = "core.monitor.name_panic_caught",
            monitor_index = idx,
        );
        Ok(format!("Monitor {}", idx))
    })
    .unwrap_or_else(|_| format!("Monitor {}", idx));
```

**Why**: The panic occurs inside xcap before the `Result` is returned, so `unwrap_or_else` on the Result cannot catch it. `catch_unwind` catches the panic at a higher level, converts it to a fallback name, and allows enumeration to continue. `AssertUnwindSafe` is appropriate here because:
1. `m` (xcap::Monitor) is consumed/moved per-iteration, no shared mutable state
2. On panic, we discard the monitor name and use a fallback — no state corruption
3. The surrounding `filter_map` already handles partial failures gracefully

### Step 2: (No test changes needed)

The existing tests already cover the relevant scenarios:
- `test_list_monitors_does_not_panic` (line 893) — will now pass on CI instead of panicking
- `test_get_monitor_not_found` (line 918) — will now correctly return `MonitorNotFound` error on CI instead of panicking
- `test_monitor_info_getters` (line 927) — tests `MonitorInfo` struct, unaffected

No new tests needed because:
1. The existing `test_list_monitors_does_not_panic` test already validates the "doesn't panic" contract
2. The existing `test_get_monitor_not_found` test already validates the error handling
3. The panic only occurs on headless macOS — we can't simulate `NSScreen.localizedName` returning NULL in a unit test without mocking the entire macOS API stack

---

## Patterns to Follow

**From codebase — property access with fallback logging:**

```rust
// SOURCE: crates/kild-peek-core/src/window/handler.rs:144-145
// Pattern for handling nullable xcap properties
let app_name = w.app_name().ok().unwrap_or_default();
let title = w.title().ok().unwrap_or_default();
```

**From codebase — monitor property error with skip:**

```rust
// SOURCE: crates/kild-peek-core/src/window/handler.rs:215-226
// Pattern for critical monitor properties that skip on failure
let x = match m.x() {
    Ok(x) => x,
    Err(e) => {
        debug!(
            event = "core.monitor.property_access_failed",
            property = "x",
            monitor_index = idx,
            error = %e
        );
        skipped_count += 1;
        return None;
    }
};
```

---

## Edge Cases & Risks

| Risk/Edge Case | Mitigation |
|----------------|------------|
| `catch_unwind` doesn't catch all panics (e.g. `abort`) | `objc2-app-kit` uses standard Rust panics, not aborts. The CI stack trace confirms a standard panic. |
| Performance cost of `catch_unwind` | Negligible — only called once per monitor during enumeration, not in a hot path. |
| Future xcap versions may fix this | The `catch_unwind` is harmless if the panic is fixed upstream — it just becomes a no-op wrapper. Can be removed when xcap is updated. |
| `AssertUnwindSafe` hiding actual problems | The wrapper is scoped narrowly to just the `m.name()` call. If xcap panics for other reasons, the fallback name is still correct behavior. |

---

## Validation

### Automated Checks

```bash
cargo fmt --check
cargo clippy -p kild-peek-core -- -D warnings
cargo test -p kild-peek-core
cargo build -p kild-peek-core
```

### Manual Verification

1. Run `cargo test -p kild-peek-core -- test_get_monitor_not_found` locally — should pass
2. Run `cargo test -p kild-peek-core -- test_list_monitors_does_not_panic` locally — should pass
3. Push to CI and verify the `macos-peek` job passes without the flaky panic

---

## Scope Boundaries

**IN SCOPE:**
- Catching the xcap/objc2-app-kit panic in `list_monitors()` at the `m.name()` call site
- Using a fallback monitor name when the panic occurs

**OUT OF SCOPE (do not touch):**
- Fixing xcap upstream (external dependency)
- Changing test structure or adding CI-only test skips
- Modifying other monitor property access patterns (x, y, width, height — these don't panic)
- Changing `MonitorInfo` type definition

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-05T12:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-235.md`
