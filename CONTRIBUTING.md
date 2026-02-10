# Contributing to KILD

## Code Formatting Standards

### Rust Code Formatting

All Rust code must be formatted using `cargo fmt` before submission:

```bash
# Format all code
cargo fmt

# Check formatting without modifying files
cargo fmt --check
```

### Pre-commit Hook

Install the pre-commit hook to automatically check formatting:

```bash
# Install pre-commit (if not already installed)
pip install pre-commit

# Install the git hook
pre-commit install
```

The hook will automatically run `cargo fmt --check` before each commit and prevent commits with formatting issues.

### CI Requirements

All PRs must pass formatting checks. The CI pipeline runs:
- `cargo fmt --check` - ensures code is properly formatted
- `cargo clippy` - linting and best practices
- `cargo test` - all tests must pass

### IDE Configuration

**VS Code**: Install the `rust-analyzer` extension and add to settings.json:
```json
{
    "rust-analyzer.rustfmt.rangeFormatting.enable": true,
    "[rust]": {
        "editor.formatOnSave": true
    }
}
```

**Other IDEs**: Configure to run `cargo fmt` on save or before commit.

## Structured Logging Convention

All log events follow the pattern: `{layer}.{domain}.{action}_{state}`

| Layer | Crate | Description |
|-------|-------|-------------|
| `cli` | `crates/kild/` | User-facing CLI commands |
| `core` | `crates/kild-core/` | Core library logic |
| `ui` | `crates/kild-ui/` | GPUI native GUI |
| `peek.cli` | `crates/kild-peek/` | kild-peek CLI commands |
| `peek.core` | `crates/kild-peek-core/` | kild-peek core library |

### Examples

```rust
// CLI layer (in crates/kild/)
info!(event = "cli.create_started", branch = branch);
info!(event = "cli.list_completed", count = sessions.len());

// Core layer (in crates/kild-core/)
info!(event = "core.session.create_completed", session_id = id);
error!(event = "core.terminal.pid_file_process_check_failed", error = %e);
info!(event = "core.git.worktree.create_started", branch = branch);

// UI layer (in crates/kild-ui/)
info!(event = "ui.watcher.started", path = %sessions_dir.display());
warn!(event = "ui.projects.add_failed", error = %e);
info!(event = "ui.terminal.pty_spawn_completed", pid = pid);
debug!(event = "ui.input.key_translated", key = key_name);

// kild-peek layers (in crates/kild-peek and kild-peek-core)
info!(event = "peek.cli.screenshot_started", window = window_name);
info!(event = "peek.core.screenshot.capture_completed", output_path = %path.display());
```

### Event Naming Guidelines

1. **Always include the layer prefix** (`cli.`, `core.`, `ui.`, `peek.cli.`, `peek.core.`)
2. **Use domain names** that match the module (e.g., `session`, `terminal`, `git`, `cleanup`, `watcher`, `projects`, `input`, `window`, `screenshot`)
3. **Use `_started`/`_completed`/`_failed` suffixes** for operation lifecycle events
4. **Sub-domains are allowed** for nested concepts (e.g., `core.git.worktree.create_started`, `peek.core.screenshot.capture_completed`)
5. **Some domains span layers** (e.g., `terminal` is used in both `core.terminal.*` for backends and `ui.terminal.*` for rendering)

### Filtering Logs by Layer

```bash
# Show only CLI events
grep '"event":"cli\.' logs.txt

# Show only core library events
grep '"event":"core\.' logs.txt

# Show only UI events
grep '"event":"ui\.' logs.txt

# Show only kild-peek events
grep '"event":"peek\.' logs.txt

# Show all failures
grep '_failed"' logs.txt

# Show watcher events
grep '"event":"ui\.watcher\.' logs.txt
```
