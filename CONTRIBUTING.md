# Contributing to Shards

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
| `cli` | `crates/shards/` | User-facing CLI commands |
| `core` | `crates/shards-core/` | Core library logic |

### Examples

```rust
// CLI layer (in crates/shards/)
info!(event = "cli.create_started", branch = branch);
info!(event = "cli.list_completed", count = sessions.len());

// Core layer (in crates/shards-core/)
info!(event = "core.session.create_completed", session_id = id);
error!(event = "core.terminal.pid_file_process_check_failed", error = %e);
info!(event = "core.git.worktree.create_started", branch = branch);
```

### Event Naming Guidelines

1. **Always include the layer prefix** (`cli.` or `core.`)
2. **Use domain names** that match the module (e.g., `session`, `terminal`, `git`, `cleanup`)
3. **Use `_started`/`_completed`/`_failed` suffixes** for operation lifecycle events
4. **Sub-domains are allowed** for nested concepts (e.g., `core.git.worktree.create_started`)

### Filtering Logs by Layer

```bash
# Show only CLI events
grep '"event":"cli\.' logs.txt

# Show only core library events
grep '"event":"core\.' logs.txt

# Show all failures
grep '_failed"' logs.txt
```
