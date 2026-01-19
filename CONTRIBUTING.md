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
