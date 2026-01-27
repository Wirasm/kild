# Installing and Updating KILD

## Prerequisites

- Rust toolchain (1.89.0+) - install via https://rustup.rs
- Git
- Supported terminal - Ghostty, iTerm2, or Terminal.app

## Install Globally

From the KILD repo root, run:

```bash
cargo install --path crates/kild
```

This compiles and installs the `kild` binary to `~/.cargo/bin/kild`.

## Verify Installation

```bash
which kild          # Should show ~/.cargo/bin/kild
kild --version      # Check version
kild list           # Test it works (run from any git repo)
```

## Update to Latest

When you pull new changes or merge PRs, update the installed binary:

```bash
# From the KILD repo
git pull origin main
cargo install --path crates/kild --force
```

The `--force` flag overwrites the existing binary with the new build.

## Quick Update One-Liner

If you want a single command to pull and reinstall:

```bash
cd /path/to/KILD && git pull origin main && cargo install --path crates/kild --force
```

## Testing Before Installing

If you want to test changes before installing globally, build and run directly:

```bash
cargo build --release --bin kild
./target/release/kild list
```

This builds but does not install - useful for testing branches before merging.

## Uninstall

```bash
cargo uninstall kild
```

## Troubleshooting

**Command not found after install**
- Ensure `~/.cargo/bin` is in your PATH
- Run `source ~/.cargo/env` or restart your terminal

**Permission denied**
- Check you have write access to `~/.cargo/bin`

**Old version still running**
- Run `which kild` to confirm path
- Use `--force` flag when reinstalling
