# Feature: Peek - Native Application Inspector CLI

## Summary

Build a standalone Rust CLI tool called `peek` that enables AI agents (and developers) to inspect, debug, and validate native macOS applications. The tool provides screenshot capture, window enumeration, accessibility tree inspection, image diffing, and assertion-based validation - all designed for AI-assisted development workflows where Claude Code needs "eyes" on native UI.

## User Story

As an AI coding agent (or developer)
I want to capture screenshots, inspect UI elements, and validate visual state of native applications
So that I can debug, verify, and self-validate my work on native GUI applications like kild-ui

## Problem Statement

AI coding agents cannot see native application UIs. When working on GPUI-based applications like kild-ui, Claude Code has no way to:
- Verify that UI changes render correctly
- Debug visual issues
- Validate that components appear as expected
- Compare before/after states of UI changes

This creates a "blind spot" where AI agents must rely entirely on user feedback for visual validation.

## Solution Statement

Create a standalone `peek` CLI crate that provides:
1. **Window discovery** - List and identify windows by title/app
2. **Screenshot capture** - Capture windows, screens, or regions
3. **Accessibility tree inspection** - Query UI element hierarchy and properties
4. **Image comparison** - Diff screenshots to detect visual changes
5. **Assertions** - Exit codes for CI/script validation of UI state
6. **AI-optimized output** - Base64 images and JSON for direct Claude consumption

The tool uses `xcap` for screenshots (simple, cross-platform) and `accessibility-sys` for macOS accessibility APIs.

## Metadata

| Field | Value |
|-------|-------|
| Type | NEW_CAPABILITY |
| Complexity | HIGH |
| Systems Affected | New standalone crate (no kild integration) |
| Dependencies | xcap 0.8, accessibility-sys 0.2, clap 4.5, image 0.25, image-compare 0.5, base64 0.22, serde 1.0, serde_json 1.0, thiserror 2.0, tracing 0.1 |
| Estimated Tasks | 25 |

---

## UX Design

### Before State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐            ║
║   │   Claude    │ ──────► │  Edit Code  │ ──────► │   ???       │            ║
║   │    Code     │         │  (kild-ui)  │         │  (No eyes)  │            ║
║   └─────────────┘         └─────────────┘         └─────────────┘            ║
║                                                                               ║
║   USER_FLOW:                                                                  ║
║   1. User asks Claude to modify kild-ui                                       ║
║   2. Claude edits GPUI code                                                   ║
║   3. Claude cannot see if changes work                                        ║
║   4. User must manually run app, check visually, report back                  ║
║   5. Cycle repeats until user confirms success                                ║
║                                                                               ║
║   PAIN_POINT: Claude is blind to native UI - cannot self-validate             ║
║                                                                               ║
║   DATA_FLOW: Code changes → Build → Run → ??? (no feedback loop)              ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### After State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐            ║
║   │   Claude    │ ──────► │  Edit Code  │ ──────► │   Build &   │            ║
║   │    Code     │         │  (kild-ui)  │         │    Run      │            ║
║   └─────────────┘         └─────────────┘         └─────────────┘            ║
║                                   │                      │                    ║
║                                   │                      ▼                    ║
║                                   │              ┌─────────────┐              ║
║                                   │              │    peek     │              ║
║                                   │              │  screenshot │              ║
║                                   │              └─────────────┘              ║
║                                   │                      │                    ║
║                                   │    ┌─────────────────┘                    ║
║                                   │    │                                      ║
║                                   ▼    ▼                                      ║
║                          ┌─────────────────┐                                  ║
║                          │  Claude reads   │  ◄── Visual feedback loop!       ║
║                          │  base64 image   │                                  ║
║                          └─────────────────┘                                  ║
║                                   │                                           ║
║                                   ▼                                           ║
║                          ┌─────────────────┐                                  ║
║                          │ Self-validates  │                                  ║
║                          │ and iterates    │                                  ║
║                          └─────────────────┘                                  ║
║                                                                               ║
║   USER_FLOW:                                                                  ║
║   1. User asks Claude to modify kild-ui                                       ║
║   2. Claude edits GPUI code                                                   ║
║   3. Claude runs: peek screenshot --window "kild-ui" --base64                 ║
║   4. Claude sees the screenshot, validates changes                            ║
║   5. Claude iterates if needed, or confirms completion                        ║
║                                                                               ║
║   VALUE_ADD: Claude can see native UI, self-validate, reduce feedback cycles  ║
║                                                                               ║
║   DATA_FLOW: Code → Build → Run → peek capture → base64 → Claude vision       ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### CLI Command Structure

```
peek
├── list                      # List windows/monitors
│   ├── windows              # List all visible windows
│   └── monitors             # List all monitors
│
├── screenshot               # Capture screenshots
│   ├── --window <TITLE>     # Capture window by title (partial match)
│   ├── --window-id <ID>     # Capture window by ID
│   ├── --monitor <INDEX>    # Capture specific monitor
│   ├── --region <X,Y,W,H>   # Capture screen region
│   ├── --output <PATH>      # Save to file (default: stdout base64)
│   ├── --base64             # Output base64 (default if no --output)
│   └── --format <FMT>       # png (default), jpg
│
├── tree                     # Accessibility tree inspection
│   ├── --window <TITLE>     # Target window
│   ├── --depth <N>          # Max depth (default: 10)
│   └── --json               # JSON output (default: tree format)
│
├── find                     # Find UI elements
│   ├── --window <TITLE>     # Target window
│   ├── --role <ROLE>        # Filter by accessibility role
│   ├── --title <TITLE>      # Filter by title (partial match)
│   ├── --label <LABEL>      # Filter by label
│   └── --json               # JSON output
│
├── inspect                  # Inspect specific element
│   ├── --window <TITLE>     # Target window
│   ├── --path <PATH>        # Element path from tree
│   └── --json               # JSON output
│
├── diff                     # Compare screenshots
│   ├── <IMAGE1>             # First image path
│   ├── <IMAGE2>             # Second image path
│   ├── --output <PATH>      # Save diff image
│   ├── --threshold <N>      # Similarity threshold (0-100, default: 95)
│   └── --json               # JSON output with metrics
│
├── assert                   # Assertions (exit codes for CI)
│   ├── --window <TITLE>     # Target window
│   ├── --exists             # Assert window exists
│   ├── --visible            # Assert window is visible
│   ├── --element-exists     # Assert element exists
│   │   ├── --role <ROLE>
│   │   ├── --title <TITLE>
│   │   └── --label <LABEL>
│   └── --similar <IMAGE>    # Assert screenshot similar to baseline
│       └── --threshold <N>
│
└── watch                    # Continuous capture (for debugging)
    ├── --window <TITLE>     # Target window
    ├── --interval <MS>      # Capture interval (default: 1000)
    ├── --output <DIR>       # Output directory
    └── --max <N>            # Max captures (default: unlimited)
```

### Interaction Changes

| Command | Action | Output | Exit Code |
|---------|--------|--------|-----------|
| `peek list windows` | Enumerate windows | Table or JSON | 0 success, 1 error |
| `peek screenshot --window "kild"` | Capture window | Base64 to stdout | 0 success, 1 not found |
| `peek screenshot --window "kild" -o shot.png` | Save screenshot | File path | 0 success, 1 error |
| `peek tree --window "kild"` | Dump accessibility tree | Tree or JSON | 0 success, 1 error |
| `peek find --window "kild" --role button` | Find buttons | Element list | 0 found, 1 none |
| `peek diff a.png b.png` | Compare images | Similarity % | 0 similar, 1 different |
| `peek assert --window "kild" --exists` | Check window | Nothing (exit code) | 0 exists, 1 missing |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/kild/src/main.rs` | 1-18 | Main entry pattern to MIRROR |
| P0 | `crates/kild/src/app.rs` | 1-100 | Clap CLI structure to MIRROR |
| P0 | `crates/kild/src/commands.rs` | 1-200 | Command handler pattern to MIRROR |
| P1 | `crates/kild-core/src/errors/mod.rs` | 1-136 | KildError trait pattern |
| P1 | `crates/kild-core/src/logging/mod.rs` | 1-24 | Logging initialization |
| P1 | `crates/kild-core/src/events/mod.rs` | 1-36 | Event logging helpers |
| P2 | `crates/kild/src/table.rs` | 1-80 | Table formatting pattern |
| P2 | `Cargo.toml` | 1-42 | Workspace structure |

**External Documentation:**

| Source | Section | Why Needed |
|--------|---------|------------|
| [xcap docs](https://docs.rs/xcap/0.8) | Window/Monitor capture | Core screenshot API |
| [accessibility-sys docs](https://docs.rs/accessibility-sys/0.2) | AXUIElement bindings | Accessibility tree queries |
| [clap docs](https://docs.rs/clap/4.5) | Derive macros | CLI argument parsing |
| [image-compare docs](https://docs.rs/image-compare/0.5) | SSIM comparison | Image diffing |

---

## Patterns to Mirror

**MAIN_RS_PATTERN:**
```rust
// SOURCE: crates/kild/src/main.rs:1-18
// COPY THIS PATTERN:
use peek_core::init_logging;

mod app;
mod commands;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = app::build_cli();
    let matches = app.get_matches();

    let quiet = matches.get_flag("quiet");
    init_logging(quiet);

    commands::run_command(&matches)?;

    Ok(())
}
```

**CLI_STRUCTURE_PATTERN:**
```rust
// SOURCE: crates/kild/src/app.rs:1-50
// COPY THIS PATTERN:
pub fn build_cli() -> Command {
    Command::new("peek")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Native application inspector for AI-assisted development")
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .help("Suppress log output")
                .action(ArgAction::SetTrue)
                .global(true),
        )
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(/* ... */)
}
```

**COMMAND_HANDLER_PATTERN:**
```rust
// SOURCE: crates/kild/src/commands.rs:59-100
// COPY THIS PATTERN:
pub fn run_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    events::log_app_startup();

    match matches.subcommand() {
        Some(("list", sub_matches)) => handle_list_command(sub_matches),
        Some(("screenshot", sub_matches)) => handle_screenshot_command(sub_matches),
        // ...
        _ => {
            error!(event = "cli.command_unknown");
            Err("Unknown command".into())
        }
    }
}

fn handle_screenshot_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let window_title = matches.get_one::<String>("window");
    let output_path = matches.get_one::<String>("output");
    let base64_output = matches.get_flag("base64") || output_path.is_none();

    info!(
        event = "cli.screenshot_started",
        window = ?window_title,
        base64 = base64_output
    );

    match screenshot_handler::capture_screenshot(/* ... */) {
        Ok(result) => {
            if base64_output {
                println!("{}", result.base64);
            } else {
                println!("Saved: {}", result.path.display());
            }
            info!(event = "cli.screenshot_completed");
            Ok(())
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            error!(event = "cli.screenshot_failed", error = %e);
            Err(e.into())
        }
    }
}
```

**ERROR_DEFINITION_PATTERN:**
```rust
// SOURCE: crates/kild-core/src/sessions/errors.rs:1-50
// COPY THIS PATTERN:
use crate::errors::PeekError;

#[derive(Debug, thiserror::Error)]
pub enum ScreenshotError {
    #[error("Window not found: '{title}'")]
    WindowNotFound { title: String },

    #[error("Window is minimized and cannot be captured: '{title}'")]
    WindowMinimized { title: String },

    #[error("Screen recording permission denied")]
    PermissionDenied,

    #[error("Image encoding failed: {0}")]
    EncodingError(String),

    #[error("IO error: {source}")]
    IoError {
        #[from]
        source: std::io::Error,
    },
}

impl PeekError for ScreenshotError {
    fn error_code(&self) -> &'static str {
        match self {
            ScreenshotError::WindowNotFound { .. } => "SCREENSHOT_WINDOW_NOT_FOUND",
            ScreenshotError::WindowMinimized { .. } => "SCREENSHOT_WINDOW_MINIMIZED",
            ScreenshotError::PermissionDenied => "SCREENSHOT_PERMISSION_DENIED",
            ScreenshotError::EncodingError(_) => "SCREENSHOT_ENCODING_ERROR",
            ScreenshotError::IoError { .. } => "SCREENSHOT_IO_ERROR",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(
            self,
            ScreenshotError::WindowNotFound { .. }
                | ScreenshotError::WindowMinimized { .. }
                | ScreenshotError::PermissionDenied
        )
    }
}
```

**LOGGING_PATTERN:**
```rust
// SOURCE: crates/kild-core/src/logging/mod.rs:1-24
// COPY THIS PATTERN:
pub fn init_logging(quiet: bool) {
    let directive = if quiet { "peek=error" } else { "peek=info" };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_writer(std::io::stderr)
                .with_current_span(false)
                .with_span_list(false),
        )
        .with(
            EnvFilter::from_default_env()
                .add_directive(directive.parse().expect("Invalid log directive")),
        )
        .init();
}
```

**JSON_OUTPUT_PATTERN:**
```rust
// SOURCE: crates/kild/src/commands.rs:151-170
// COPY THIS PATTERN:
fn handle_list_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let json_output = matches.get_flag("json");

    match window_handler::list_windows() {
        Ok(windows) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&windows)?);
            } else {
                for window in &windows {
                    println!("  [{:>4}] {} ({}x{})",
                        window.id, window.title, window.width, window.height);
                }
            }
            Ok(())
        }
        Err(e) => { /* ... */ }
    }
}
```

---

## Files to Change

### New Crate: `crates/peek`

| File | Action | Justification |
|------|--------|---------------|
| `crates/peek/Cargo.toml` | CREATE | CLI crate manifest |
| `crates/peek/src/main.rs` | CREATE | Entry point |
| `crates/peek/src/app.rs` | CREATE | Clap CLI definition |
| `crates/peek/src/commands.rs` | CREATE | Command handlers |
| `crates/peek/src/table.rs` | CREATE | Human-readable output formatting |

### New Crate: `crates/peek-core`

| File | Action | Justification |
|------|--------|---------------|
| `crates/peek-core/Cargo.toml` | CREATE | Core library manifest |
| `crates/peek-core/src/lib.rs` | CREATE | Library root, public exports |
| `crates/peek-core/src/errors/mod.rs` | CREATE | PeekError trait and base errors |
| `crates/peek-core/src/logging/mod.rs` | CREATE | Tracing initialization |
| `crates/peek-core/src/events/mod.rs` | CREATE | App lifecycle event helpers |
| `crates/peek-core/src/window/mod.rs` | CREATE | Window module root |
| `crates/peek-core/src/window/types.rs` | CREATE | WindowInfo, MonitorInfo types |
| `crates/peek-core/src/window/errors.rs` | CREATE | Window enumeration errors |
| `crates/peek-core/src/window/handler.rs` | CREATE | list_windows(), list_monitors() |
| `crates/peek-core/src/screenshot/mod.rs` | CREATE | Screenshot module root |
| `crates/peek-core/src/screenshot/types.rs` | CREATE | CaptureRequest, CaptureResult |
| `crates/peek-core/src/screenshot/errors.rs` | CREATE | ScreenshotError |
| `crates/peek-core/src/screenshot/handler.rs` | CREATE | capture_window(), capture_monitor() |
| `crates/peek-core/src/accessibility/mod.rs` | CREATE | Accessibility module root |
| `crates/peek-core/src/accessibility/types.rs` | CREATE | ElementInfo, ElementTree |
| `crates/peek-core/src/accessibility/errors.rs` | CREATE | AccessibilityError |
| `crates/peek-core/src/accessibility/handler.rs` | CREATE | get_tree(), find_elements() |
| `crates/peek-core/src/diff/mod.rs` | CREATE | Image diff module root |
| `crates/peek-core/src/diff/types.rs` | CREATE | DiffRequest, DiffResult |
| `crates/peek-core/src/diff/errors.rs` | CREATE | DiffError |
| `crates/peek-core/src/diff/handler.rs` | CREATE | compare_images() |
| `crates/peek-core/src/assert/mod.rs` | CREATE | Assertion module root |
| `crates/peek-core/src/assert/types.rs` | CREATE | AssertionRequest, AssertionResult |
| `crates/peek-core/src/assert/errors.rs` | CREATE | AssertionError |
| `crates/peek-core/src/assert/handler.rs` | CREATE | assert_window(), assert_element() |

### Workspace Update

| File | Action | Justification |
|------|--------|---------------|
| `Cargo.toml` | UPDATE | Add peek, peek-core to workspace members and deps |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **No OCR** - Claude's vision handles text extraction; adding Tesseract/similar adds complexity
- **No video recording** - Screenshots only; video would require different architecture
- **No input simulation** - Read-only inspection tool; no clicking, typing, or automation
- **No cross-platform** - macOS only for accessibility APIs; xcap handles cross-platform screenshots but we don't test Linux/Windows
- **No MCP server** - CLI first; MCP wrapper can be built later as separate crate
- **No watch mode in v1** - Continuous capture is nice-to-have, defer to v2
- **No element interaction** - Cannot click buttons, focus elements; read-only inspection
- **No GPUI-specific hooks** - Use standard macOS accessibility; no GPUI introspection

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Phase 1: Project Setup

#### Task 1: UPDATE `Cargo.toml` (workspace root)

- **ACTION**: Add peek and peek-core to workspace
- **IMPLEMENT**:
  ```toml
  [workspace]
  members = ["crates/*"]

  [workspace.dependencies]
  # Add new deps
  xcap = "0.8"
  accessibility-sys = "0.2"
  image-compare = "0.5"
  base64 = "0.22"
  # Existing deps already present: clap, thiserror, tracing, serde, serde_json, image
  peek-core = { path = "crates/peek-core" }
  ```
- **MIRROR**: Existing workspace.dependencies pattern
- **VALIDATE**: `cargo check --workspace`

#### Task 2: CREATE `crates/peek-core/Cargo.toml`

- **ACTION**: Create core library manifest
- **IMPLEMENT**:
  ```toml
  [package]
  name = "peek-core"
  version.workspace = true
  edition.workspace = true
  license.workspace = true

  [dependencies]
  thiserror.workspace = true
  tracing.workspace = true
  tracing-subscriber = { workspace = true, features = ["json", "env-filter"] }
  serde = { workspace = true, features = ["derive"] }
  serde_json.workspace = true
  xcap.workspace = true
  accessibility-sys.workspace = true
  image.workspace = true
  image-compare.workspace = true
  base64.workspace = true
  ```
- **MIRROR**: `crates/kild-core/Cargo.toml` structure
- **VALIDATE**: `cargo check -p peek-core`

#### Task 3: CREATE `crates/peek/Cargo.toml`

- **ACTION**: Create CLI crate manifest
- **IMPLEMENT**:
  ```toml
  [package]
  name = "peek"
  version.workspace = true
  edition.workspace = true
  license.workspace = true

  [[bin]]
  name = "peek"
  path = "src/main.rs"

  [dependencies]
  peek-core.workspace = true
  clap.workspace = true
  tracing.workspace = true
  serde_json.workspace = true
  ```
- **MIRROR**: `crates/kild/Cargo.toml` structure
- **VALIDATE**: `cargo check -p peek`

### Phase 2: Core Infrastructure

#### Task 4: CREATE `crates/peek-core/src/errors/mod.rs`

- **ACTION**: Define PeekError trait and base error infrastructure
- **IMPLEMENT**:
  - `PeekError` trait with `error_code()` and `is_user_error()`
  - `PeekResult<T>` type alias
  - Base error handling utilities
- **MIRROR**: `crates/kild-core/src/errors/mod.rs:1-136`
- **VALIDATE**: `cargo check -p peek-core`

#### Task 5: CREATE `crates/peek-core/src/logging/mod.rs`

- **ACTION**: Tracing initialization with JSON output
- **IMPLEMENT**: `init_logging(quiet: bool)` function
- **MIRROR**: `crates/kild-core/src/logging/mod.rs:1-24`
- **VALIDATE**: `cargo check -p peek-core`

#### Task 6: CREATE `crates/peek-core/src/events/mod.rs`

- **ACTION**: App lifecycle event helpers
- **IMPLEMENT**: `log_app_startup()`, `log_app_shutdown()`, `log_app_error()`
- **MIRROR**: `crates/kild-core/src/events/mod.rs:1-36`
- **VALIDATE**: `cargo check -p peek-core`

#### Task 7: CREATE `crates/peek-core/src/lib.rs`

- **ACTION**: Library root with public exports
- **IMPLEMENT**:
  ```rust
  pub mod errors;
  pub mod events;
  pub mod logging;
  pub mod window;
  pub mod screenshot;
  pub mod accessibility;
  pub mod diff;
  pub mod assert;

  pub use errors::{PeekError, PeekResult};
  pub use logging::init_logging;
  ```
- **VALIDATE**: `cargo check -p peek-core`

### Phase 3: Window Module

#### Task 8: CREATE `crates/peek-core/src/window/types.rs`

- **ACTION**: Window and monitor type definitions
- **IMPLEMENT**:
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct WindowInfo {
      pub id: u32,
      pub title: String,
      pub app_name: String,
      pub x: i32,
      pub y: i32,
      pub width: u32,
      pub height: u32,
      pub is_minimized: bool,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct MonitorInfo {
      pub id: u32,
      pub name: String,
      pub x: i32,
      pub y: i32,
      pub width: u32,
      pub height: u32,
      pub is_primary: bool,
  }
  ```
- **VALIDATE**: `cargo check -p peek-core`

#### Task 9: CREATE `crates/peek-core/src/window/errors.rs`

- **ACTION**: Window enumeration errors
- **IMPLEMENT**: `WindowError` enum with variants for enumeration failures
- **MIRROR**: Error pattern from Task 4
- **VALIDATE**: `cargo check -p peek-core`

#### Task 10: CREATE `crates/peek-core/src/window/handler.rs`

- **ACTION**: Window listing operations using xcap
- **IMPLEMENT**:
  ```rust
  pub fn list_windows() -> Result<Vec<WindowInfo>, WindowError> {
      info!(event = "core.window.list_started");
      let windows = xcap::Window::all()?;
      // Transform to WindowInfo, filter minimized
      info!(event = "core.window.list_completed", count = result.len());
      Ok(result)
  }

  pub fn list_monitors() -> Result<Vec<MonitorInfo>, WindowError> { /* ... */ }

  pub fn find_window_by_title(title: &str) -> Result<WindowInfo, WindowError> { /* ... */ }
  ```
- **VALIDATE**: `cargo test -p peek-core window`

#### Task 11: CREATE `crates/peek-core/src/window/mod.rs`

- **ACTION**: Module root with exports
- **IMPLEMENT**: Re-export types, errors, handler functions
- **VALIDATE**: `cargo check -p peek-core`

### Phase 4: Screenshot Module

#### Task 12: CREATE `crates/peek-core/src/screenshot/types.rs`

- **ACTION**: Screenshot request/result types
- **IMPLEMENT**:
  ```rust
  #[derive(Debug, Clone)]
  pub enum CaptureTarget {
      Window { title: String },
      WindowId { id: u32 },
      Monitor { index: usize },
      Region { x: i32, y: i32, width: u32, height: u32 },
      PrimaryMonitor,
  }

  #[derive(Debug, Clone)]
  pub struct CaptureRequest {
      pub target: CaptureTarget,
      pub format: ImageFormat,
  }

  #[derive(Debug, Clone, Default)]
  pub enum ImageFormat {
      #[default]
      Png,
      Jpeg { quality: u8 },
  }

  #[derive(Debug)]
  pub struct CaptureResult {
      pub width: u32,
      pub height: u32,
      pub format: ImageFormat,
      pub data: Vec<u8>,  // Encoded image bytes
  }

  impl CaptureResult {
      pub fn to_base64(&self) -> String {
          base64::engine::general_purpose::STANDARD.encode(&self.data)
      }
  }
  ```
- **VALIDATE**: `cargo check -p peek-core`

#### Task 13: CREATE `crates/peek-core/src/screenshot/errors.rs`

- **ACTION**: Screenshot-specific errors
- **IMPLEMENT**: WindowNotFound, WindowMinimized, PermissionDenied, EncodingError, IoError
- **MIRROR**: Error pattern from Task 4
- **VALIDATE**: `cargo check -p peek-core`

#### Task 14: CREATE `crates/peek-core/src/screenshot/handler.rs`

- **ACTION**: Screenshot capture operations using xcap
- **IMPLEMENT**:
  ```rust
  pub fn capture(request: &CaptureRequest) -> Result<CaptureResult, ScreenshotError> {
      info!(event = "core.screenshot.capture_started", target = ?request.target);

      match &request.target {
          CaptureTarget::Window { title } => capture_window_by_title(title, &request.format),
          CaptureTarget::WindowId { id } => capture_window_by_id(*id, &request.format),
          CaptureTarget::Monitor { index } => capture_monitor(*index, &request.format),
          CaptureTarget::Region { x, y, width, height } => capture_region(*x, *y, *width, *height, &request.format),
          CaptureTarget::PrimaryMonitor => capture_primary_monitor(&request.format),
      }
  }

  fn capture_window_by_title(title: &str, format: &ImageFormat) -> Result<CaptureResult, ScreenshotError> {
      let windows = xcap::Window::all().map_err(|e| ScreenshotError::EnumerationFailed(e.to_string()))?;

      let window = windows.iter()
          .find(|w| w.title().unwrap_or_default().contains(title))
          .ok_or_else(|| ScreenshotError::WindowNotFound { title: title.to_string() })?;

      if window.is_minimized().unwrap_or(false) {
          return Err(ScreenshotError::WindowMinimized { title: title.to_string() });
      }

      let image = window.capture_image().map_err(|e| ScreenshotError::CaptureFailed(e.to_string()))?;
      encode_image(image, format)
  }
  ```
- **GOTCHA**: xcap returns `image::DynamicImage`, encode to PNG/JPEG bytes
- **VALIDATE**: `cargo test -p peek-core screenshot`

#### Task 15: CREATE `crates/peek-core/src/screenshot/mod.rs`

- **ACTION**: Module root with exports
- **VALIDATE**: `cargo check -p peek-core`

### Phase 5: Accessibility Module

#### Task 16: CREATE `crates/peek-core/src/accessibility/types.rs`

- **ACTION**: Accessibility element types
- **IMPLEMENT**:
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct ElementInfo {
      pub role: String,
      pub title: Option<String>,
      pub label: Option<String>,
      pub value: Option<String>,
      pub description: Option<String>,
      pub bounds: Option<ElementBounds>,
      pub is_enabled: bool,
      pub is_focused: bool,
      pub children_count: usize,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct ElementBounds {
      pub x: f64,
      pub y: f64,
      pub width: f64,
      pub height: f64,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct ElementTree {
      pub root: ElementNode,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct ElementNode {
      pub info: ElementInfo,
      pub children: Vec<ElementNode>,
      pub path: String,  // e.g., "0/2/1" for navigation
  }

  #[derive(Debug, Clone)]
  pub struct ElementQuery {
      pub role: Option<String>,
      pub title: Option<String>,
      pub label: Option<String>,
  }
  ```
- **VALIDATE**: `cargo check -p peek-core`

#### Task 17: CREATE `crates/peek-core/src/accessibility/errors.rs`

- **ACTION**: Accessibility-specific errors
- **IMPLEMENT**: PermissionDenied, WindowNotFound, ElementNotFound, ApiError
- **VALIDATE**: `cargo check -p peek-core`

#### Task 18: CREATE `crates/peek-core/src/accessibility/handler.rs`

- **ACTION**: Accessibility tree operations using accessibility-sys
- **IMPLEMENT**:
  ```rust
  pub fn get_window_tree(window_title: &str, max_depth: usize) -> Result<ElementTree, AccessibilityError> {
      info!(event = "core.accessibility.tree_started", window = window_title, depth = max_depth);

      // 1. Find window PID from title
      // 2. Create AXUIElementCreateApplication for PID
      // 3. Recursively traverse children up to max_depth
      // 4. Build ElementTree structure

      info!(event = "core.accessibility.tree_completed", elements = tree.count());
      Ok(tree)
  }

  pub fn find_elements(window_title: &str, query: &ElementQuery) -> Result<Vec<ElementInfo>, AccessibilityError> {
      // Get tree, filter by query criteria
  }

  pub fn get_element_at_path(window_title: &str, path: &str) -> Result<ElementInfo, AccessibilityError> {
      // Navigate tree by path string
  }
  ```
- **GOTCHA**: accessibility-sys is unsafe FFI; wrap carefully with error handling
- **GOTCHA**: Requires Accessibility permission in System Settings
- **VALIDATE**: `cargo test -p peek-core accessibility`

#### Task 19: CREATE `crates/peek-core/src/accessibility/mod.rs`

- **ACTION**: Module root with exports
- **VALIDATE**: `cargo check -p peek-core`

### Phase 6: Diff Module

#### Task 20: CREATE `crates/peek-core/src/diff/types.rs`

- **ACTION**: Image comparison types
- **IMPLEMENT**:
  ```rust
  #[derive(Debug, Clone)]
  pub struct DiffRequest {
      pub image1_path: PathBuf,
      pub image2_path: PathBuf,
      pub threshold: f64,  // 0.0 - 1.0, default 0.95
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct DiffResult {
      pub similarity: f64,  // 0.0 - 1.0
      pub is_similar: bool, // similarity >= threshold
      pub width: u32,
      pub height: u32,
      pub diff_pixels: u64,
  }
  ```
- **VALIDATE**: `cargo check -p peek-core`

#### Task 21: CREATE `crates/peek-core/src/diff/errors.rs`

- **ACTION**: Diff-specific errors
- **IMPLEMENT**: ImageLoadFailed, DimensionMismatch, ComparisonFailed
- **VALIDATE**: `cargo check -p peek-core`

#### Task 22: CREATE `crates/peek-core/src/diff/handler.rs`

- **ACTION**: Image comparison using image-compare
- **IMPLEMENT**:
  ```rust
  pub fn compare_images(request: &DiffRequest) -> Result<DiffResult, DiffError> {
      info!(event = "core.diff.compare_started",
          image1 = %request.image1_path.display(),
          image2 = %request.image2_path.display());

      let img1 = image::open(&request.image1_path)?;
      let img2 = image::open(&request.image2_path)?;

      // Check dimensions match
      if img1.dimensions() != img2.dimensions() {
          return Err(DiffError::DimensionMismatch { /* ... */ });
      }

      // Use SSIM (Structural Similarity Index)
      let result = image_compare::ssim(&img1.to_rgb8(), &img2.to_rgb8())?;

      let diff_result = DiffResult {
          similarity: result.score,
          is_similar: result.score >= request.threshold,
          // ...
      };

      info!(event = "core.diff.compare_completed", similarity = diff_result.similarity);
      Ok(diff_result)
  }

  pub fn generate_diff_image(request: &DiffRequest, output_path: &Path) -> Result<DiffResult, DiffError> {
      // Compare and generate visual diff highlighting differences
  }
  ```
- **VALIDATE**: `cargo test -p peek-core diff`

#### Task 23: CREATE `crates/peek-core/src/diff/mod.rs`

- **ACTION**: Module root with exports
- **VALIDATE**: `cargo check -p peek-core`

### Phase 7: Assert Module

#### Task 24: CREATE `crates/peek-core/src/assert/types.rs`

- **ACTION**: Assertion request/result types
- **IMPLEMENT**:
  ```rust
  #[derive(Debug, Clone)]
  pub enum Assertion {
      WindowExists { title: String },
      WindowVisible { title: String },
      ElementExists { window_title: String, query: ElementQuery },
      ImageSimilar { image_path: PathBuf, baseline_path: PathBuf, threshold: f64 },
  }

  #[derive(Debug, Clone)]
  pub struct AssertionResult {
      pub passed: bool,
      pub message: String,
      pub details: Option<serde_json::Value>,
  }
  ```
- **VALIDATE**: `cargo check -p peek-core`

#### Task 25: CREATE `crates/peek-core/src/assert/errors.rs`

- **ACTION**: Assertion-specific errors
- **IMPLEMENT**: AssertionFailed with details
- **VALIDATE**: `cargo check -p peek-core`

#### Task 26: CREATE `crates/peek-core/src/assert/handler.rs`

- **ACTION**: Assertion execution
- **IMPLEMENT**:
  ```rust
  pub fn run_assertion(assertion: &Assertion) -> Result<AssertionResult, AssertError> {
      info!(event = "core.assert.run_started", assertion = ?assertion);

      let result = match assertion {
          Assertion::WindowExists { title } => assert_window_exists(title),
          Assertion::WindowVisible { title } => assert_window_visible(title),
          Assertion::ElementExists { window_title, query } => assert_element_exists(window_title, query),
          Assertion::ImageSimilar { image_path, baseline_path, threshold } => {
              assert_image_similar(image_path, baseline_path, *threshold)
          }
      }?;

      if result.passed {
          info!(event = "core.assert.run_passed", message = %result.message);
      } else {
          warn!(event = "core.assert.run_failed", message = %result.message);
      }

      Ok(result)
  }
  ```
- **VALIDATE**: `cargo test -p peek-core assert`

#### Task 27: CREATE `crates/peek-core/src/assert/mod.rs`

- **ACTION**: Module root with exports
- **VALIDATE**: `cargo check -p peek-core`

### Phase 8: CLI Implementation

#### Task 28: CREATE `crates/peek/src/main.rs`

- **ACTION**: CLI entry point
- **IMPLEMENT**: Mirror kild main.rs pattern exactly
- **MIRROR**: `crates/kild/src/main.rs:1-18`
- **VALIDATE**: `cargo build -p peek`

#### Task 29: CREATE `crates/peek/src/app.rs`

- **ACTION**: Full clap CLI definition
- **IMPLEMENT**: All subcommands from CLI structure above
  - `list` with `windows` and `monitors` subcommands
  - `screenshot` with all options
  - `tree` for accessibility tree
  - `find` for element search
  - `inspect` for element details
  - `diff` for image comparison
  - `assert` for assertions
- **MIRROR**: `crates/kild/src/app.rs` structure
- **VALIDATE**: `cargo build -p peek`

#### Task 30: CREATE `crates/peek/src/commands.rs`

- **ACTION**: All command handlers
- **IMPLEMENT**: Handler for each subcommand
  - `handle_list_command()`
  - `handle_screenshot_command()`
  - `handle_tree_command()`
  - `handle_find_command()`
  - `handle_inspect_command()`
  - `handle_diff_command()`
  - `handle_assert_command()`
- **MIRROR**: `crates/kild/src/commands.rs` pattern
- **VALIDATE**: `cargo build -p peek && cargo run -p peek -- --help`

#### Task 31: CREATE `crates/peek/src/table.rs`

- **ACTION**: Human-readable table formatting
- **IMPLEMENT**: Formatters for window list, element list, diff results
- **MIRROR**: `crates/kild/src/table.rs`
- **VALIDATE**: `cargo build -p peek`

### Phase 9: Testing

#### Task 32: CREATE `crates/peek/tests/cli_output.rs`

- **ACTION**: Integration tests for CLI output
- **IMPLEMENT**:
  - Test `peek list windows` succeeds
  - Test `peek --help` shows all commands
  - Test quiet mode suppresses logs
  - Test JSON output format
- **MIRROR**: `crates/kild/tests/cli_output.rs`
- **VALIDATE**: `cargo test -p peek`

#### Task 33: ADD unit tests to all modules

- **ACTION**: Add `#[cfg(test)]` modules with tests
- **IMPLEMENT**: Tests for each handler, error cases, edge cases
- **VALIDATE**: `cargo test --all`

### Phase 10: Documentation & Polish

#### Task 34: UPDATE `CLAUDE.md`

- **ACTION**: Document peek in CLAUDE.md
- **IMPLEMENT**: Add to Architecture section, add run commands
- **VALIDATE**: Review documentation is accurate

#### Task 35: Final validation

- **ACTION**: Run full validation suite
- **VALIDATE**:
  ```bash
  cargo fmt --check
  cargo clippy --all -- -D warnings
  cargo test --all
  cargo build --all
  cargo run -p peek -- list windows
  cargo run -p peek -- --help
  ```

---

## Testing Strategy

### Unit Tests to Write

| Test File | Test Cases | Validates |
|-----------|------------|-----------|
| `peek-core/src/window/handler.rs` | list_windows returns valid data, find_window partial match | Window enumeration |
| `peek-core/src/screenshot/handler.rs` | capture valid window, capture missing window errors, encode PNG/JPEG | Screenshot capture |
| `peek-core/src/accessibility/handler.rs` | get_tree returns structure, find_elements filters correctly | Accessibility APIs |
| `peek-core/src/diff/handler.rs` | identical images score 1.0, different images score lower | Image comparison |
| `peek-core/src/assert/handler.rs` | assertions return correct pass/fail | Assertion logic |

### Edge Cases Checklist

- [ ] Window title not found → WindowNotFound error with title
- [ ] Window is minimized → WindowMinimized error
- [ ] Screen recording permission denied → PermissionDenied error
- [ ] Accessibility permission denied → PermissionDenied error
- [ ] Image dimensions mismatch in diff → DimensionMismatch error
- [ ] Invalid image path → IoError
- [ ] Empty window list → Empty result (not error)
- [ ] Partial title match → Finds window
- [ ] Multiple windows match title → Returns first match (document behavior)
- [ ] Deep accessibility tree → Respects max_depth

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo fmt --check && cargo clippy --all -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: UNIT_TESTS

```bash
cargo test --all
```

**EXPECT**: All tests pass

### Level 3: FULL_SUITE

```bash
cargo test --all && cargo build --all
```

**EXPECT**: All tests pass, build succeeds

### Level 4: MANUAL_VALIDATION

1. Open any window (e.g., Terminal, Safari)
2. Run: `cargo run -p peek -- list windows`
   - Verify window appears in list
3. Run: `cargo run -p peek -- screenshot --window "Terminal" -o /tmp/test.png`
   - Verify screenshot saved and shows Terminal window
4. Run: `cargo run -p peek -- screenshot --window "Terminal" --base64`
   - Verify base64 string output
5. Run: `cargo run -p peek -- tree --window "Terminal"`
   - Verify accessibility tree shows elements
6. Run: `cargo run -p peek -- assert --window "Terminal" --exists`
   - Verify exit code 0
7. Run: `cargo run -p peek -- assert --window "NonExistent" --exists`
   - Verify exit code 1

---

## Acceptance Criteria

- [ ] `peek list windows` shows all visible windows with title, dimensions
- [ ] `peek list monitors` shows all monitors with resolution
- [ ] `peek screenshot --window <title>` captures correct window
- [ ] `peek screenshot --base64` outputs valid base64 PNG
- [ ] `peek screenshot -o <path>` saves file to specified path
- [ ] `peek tree --window <title>` shows accessibility hierarchy
- [ ] `peek tree --json` outputs valid JSON
- [ ] `peek find --role button` finds all buttons in window
- [ ] `peek diff` compares images and reports similarity
- [ ] `peek assert --exists` returns exit 0 when window exists
- [ ] `peek assert --exists` returns exit 1 when window missing
- [ ] All commands support `-q` quiet flag
- [ ] JSON output mode works on all applicable commands
- [ ] Errors include helpful context (window title, error code)

---

## Completion Checklist

- [ ] All 35 tasks completed in dependency order
- [ ] Level 1: Static analysis passes
- [ ] Level 2: Unit tests pass
- [ ] Level 3: Full build succeeds
- [ ] Level 4: Manual validation passes
- [ ] All acceptance criteria met
- [ ] CLAUDE.md updated with peek documentation

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Screen Recording permission UX | HIGH | MEDIUM | Clear error message with instructions to enable in System Settings |
| Accessibility permission UX | HIGH | MEDIUM | Clear error message with instructions to enable in System Settings |
| xcap doesn't work with certain windows | MEDIUM | LOW | Document limitations; some apps may not be capturable |
| accessibility-sys FFI complexity | MEDIUM | HIGH | Wrap all unsafe code carefully; extensive error handling |
| GPUI windows may not expose accessibility | MEDIUM | HIGH | Test with actual kild-ui; may need to add accessibility support to GPUI components |
| Image comparison false positives | LOW | LOW | Adjustable threshold; document SSIM behavior |

---

## Notes

### Design Decisions

1. **Standalone crate**: Not integrated with kild-core because peek is a general-purpose developer tool, not kild-specific
2. **Core + CLI split**: Follows kild pattern for testability and potential future MCP integration
3. **xcap over screencapturekit-rs**: Simpler API, sufficient for screenshot needs
4. **accessibility-sys over higher-level crates**: More control, better for specific queries
5. **SSIM for image comparison**: Standard algorithm, good for visual similarity
6. **Exit codes for assertions**: Enables use in CI/scripts

### Future Enhancements (v2)

- Watch mode for continuous capture
- MCP server wrapper for Claude Desktop integration
- Element highlighting in screenshots
- Video recording for debugging animations
- Cross-platform support (Linux/Windows)
- GPUI-specific introspection if GPUI DevTools lands

### Permission Handling

First run will trigger macOS permission dialogs:
- **Screen Recording**: Required for screenshots
- **Accessibility**: Required for element inspection

Both must be granted in System Settings → Privacy & Security. The CLI should detect permission denial and provide clear instructions.
