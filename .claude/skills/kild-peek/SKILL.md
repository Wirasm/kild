---
name: kild-peek
description: |
  Capture screenshots and inspect native application windows for visual verification.

  TRIGGERS - Use this skill when user says:
  - "peek at X", "peek on X", "use kild-peek"
  - "take a screenshot of", "capture the window"
  - "look at the UI", "take a look at the window", "see what X looks like"
  - "visual check", "verify the UI shows", "check the window"
  - "compare screenshots", "diff the images"
  - "assert the window", "validate the UI"

  kild-peek captures screenshots of native macOS applications, compares images,
  and runs assertions on UI state. Use it to visually verify what applications
  look like and validate UI state during development.

  IMPORTANT: Always list windows first to identify the correct target.
  Save screenshots to the scratchpad directory for easy cleanup.

allowed-tools: Bash, Read, Glob, Grep
---

# kild-peek CLI - Native Application Inspector

kild-peek captures screenshots, lists windows, compares images, and validates UI state for native macOS applications.

## Running kild-peek

**If installed globally:**
```bash
kild-peek list windows
```

**During development (via cargo):**
```bash
cargo run -p kild-peek -- list windows
```

The examples below use `kild-peek` directly. Prefix with `cargo run -p kild-peek --` if not installed.

## Important: Identify the Correct Window First

**Always list windows before capturing** to identify the correct target:

```bash
kild-peek list windows
```

This shows all visible windows with:
- **ID** - Unique window identifier (use with `--window-id`)
- **Title** - Window title (use with `--window`)
- **App** - Application name (use with `--app` for filtering)
- **Size** - Window dimensions
- **Status** - Visible or Minimized

### Window Matching Strategies

**Using `--window <title>` (title-based matching):**
1. Exact case-insensitive match on window title
2. Exact case-insensitive match on app name
3. Partial case-insensitive match on window title
4. Partial case-insensitive match on app name

**Using `--app <name>` (app-based matching):**
Finds windows by application name. More reliable than title matching when multiple windows share similar titles.

**Using `--app` + `--window` (precise matching):**
Combines both filters for exact targeting when multiple windows exist for the same app.

### Matching User Intent to Windows

When the user asks to "peek at X", find the right window:

| User Says | Search Strategy |
|-----------|-----------------|
| "the terminal" | Search for Ghostty, iTerm, Terminal |
| "my editor" | Search for Zed, VS Code, Cursor |
| "the kild UI" | Search for "KILD" or "kild-ui" |
| "the browser" | Search for Safari, Chrome, Firefox, Arc |
| "app X" | Search for X in title or app name |

**If multiple matches exist**, use `--window-id` with the specific ID, or ask the user to clarify.

## Screenshot Storage

**Always save screenshots to the scratchpad directory:**

```bash
# Save screenshots to the scratchpad
kild-peek screenshot --window "KILD" -o "$SCRATCHPAD/kild-ui.png"
```

This keeps screenshots organized and easy to clean up.

## Core Commands

### List Windows
```bash
kild-peek list windows [--app <name>] [--json]
```

Shows all visible windows. Always run this first to identify targets.

**Flags:**
- `--app <name>` - Filter windows by application name
- `--json` - Output as JSON

**Examples:**
```bash
# Human-readable table
kild-peek list windows

# Filter by app
kild-peek list windows --app Ghostty
kild-peek list windows --app "Visual Studio Code"

# JSON for parsing
kild-peek list windows --json

# Find specific window with JSON
kild-peek list windows --json | grep -i "terminal"
```

### List Monitors
```bash
kild-peek list monitors [--json]
```

Shows all connected displays.

### Capture Screenshot
```bash
kild-peek screenshot [--window <title>] [--app <name>] [--window-id <id>] [--monitor <index>] -o <path>
```

Captures a screenshot of a window or monitor.

**Flags:**
- `--window <title>` - Capture window by title (exact match preferred, falls back to partial)
- `--app <name>` - Capture window by app name (can combine with `--window` for precision)
- `--window-id <id>` - Capture window by exact ID
- `--monitor <index>` - Capture specific monitor (0 = primary)
- `-o <path>` - Output file path (required for file output)
- `--format <png|jpg>` - Image format (default: png)
- `--quality <1-100>` - JPEG quality (default: 85)
- `--base64` - Output base64 to stdout instead of file

**Note:** `--app`, `--window-id`, and `--monitor` are mutually exclusive. You can combine `--app` with `--window` for precise matching.

**Examples:**
```bash
# Capture by window title
kild-peek screenshot --window "KILD" -o "$SCRATCHPAD/kild.png"

# Capture by app name (more reliable than title)
kild-peek screenshot --app Ghostty -o "$SCRATCHPAD/ghostty.png"

# Combine app and title for precision
kild-peek screenshot --app Ghostty --window "Terminal" -o "$SCRATCHPAD/precise.png"

# Capture by window ID (most precise)
kild-peek screenshot --window-id 8002 -o "$SCRATCHPAD/window.png"

# Capture primary monitor
kild-peek screenshot -o "$SCRATCHPAD/screen.png"

# Capture as JPEG
kild-peek screenshot --window "Terminal" -o "$SCRATCHPAD/term.jpg" --format jpg --quality 90
```

### Compare Images (Diff)
```bash
kild-peek diff <image1> <image2> [--threshold <0-100>] [--json]
```

Compares two images using SSIM (Structural Similarity Index).

**Flags:**
- `--threshold <0-100>` - Similarity threshold percentage (default: 95)
- `--json` - Output result as JSON

**Exit codes:**
- `0` - Images are similar (meet threshold)
- `1` - Images are different (below threshold)

**Examples:**
```bash
# Compare with default 95% threshold
kild-peek diff "$SCRATCHPAD/before.png" "$SCRATCHPAD/after.png"

# Compare with lower threshold (more lenient)
kild-peek diff "$SCRATCHPAD/a.png" "$SCRATCHPAD/b.png" --threshold 80

# JSON output for scripting
kild-peek diff "$SCRATCHPAD/a.png" "$SCRATCHPAD/b.png" --json
```

### Assert UI State
```bash
kild-peek assert [--window <title>] [--app <name>] [--exists|--visible|--similar <baseline>] [--json]
```

Runs assertions on UI state. Returns exit code 0 for pass, 1 for fail.

**Assertion types:**
- `--exists` - Assert window exists
- `--visible` - Assert window is visible (not minimized)
- `--similar <path>` - Assert current screenshot matches baseline image

**Flags:**
- `--window <title>` - Target window by title
- `--app <name>` - Target window by app name (can combine with `--window`)
- `--threshold <0-100>` - Similarity threshold for `--similar` (default: 95)
- `--json` - Output result as JSON

**Examples:**
```bash
# Assert window exists by title
kild-peek assert --window "KILD" --exists

# Assert window exists by app (more reliable)
kild-peek assert --app "KILD" --exists

# Assert window is visible
kild-peek assert --window "Terminal" --visible

# Precise targeting with app + title
kild-peek assert --app Ghostty --window "Terminal" --visible

# Assert UI matches baseline
kild-peek assert --window "KILD" --similar "$SCRATCHPAD/baseline.png" --threshold 90

# JSON output
kild-peek assert --window "KILD" --exists --json
```

## Workflow Examples

### Visual Verification of UI Changes

```bash
# 1. List windows to find target
kild-peek list windows --app KILD

# 2. Capture before state (using app for reliability)
kild-peek screenshot --app KILD -o "$SCRATCHPAD/before.png"

# 3. Make changes...

# 4. Capture after state
kild-peek screenshot --app KILD -o "$SCRATCHPAD/after.png"

# 5. Compare
kild-peek diff "$SCRATCHPAD/before.png" "$SCRATCHPAD/after.png" --threshold 80
```

### Validate UI State in Tests

```bash
# Assert the KILD UI is running and visible (using app for reliability)
kild-peek assert --app KILD --visible

# Assert it matches expected appearance
kild-peek assert --app KILD --similar "./baselines/kild-empty-state.png" --threshold 90
```

### Capture Multiple Windows

```bash
# List all windows
kild-peek list windows --json > "$SCRATCHPAD/windows.json"

# Capture specific ones by ID
kild-peek screenshot --window-id 8002 -o "$SCRATCHPAD/kild.png"
kild-peek screenshot --window-id 8429 -o "$SCRATCHPAD/ghostty.png"
```

## Tips

1. **Output is clean by default** - JSON logs are suppressed unless you use `-v/--verbose`
2. **List windows first** to identify the correct target before capturing
3. **Use `--app` for reliability** when multiple windows have similar titles
4. **Combine `--app` and `--window`** for precise targeting when needed
5. **Use `--window-id`** for the most precise targeting (unique window ID)
6. **Save to scratchpad** for easy cleanup of temporary screenshots
7. **Use `--json`** for scripting and parsing results programmatically
8. **Exit codes are meaningful** - use them in shell scripts for automation

## Global Flags

- `-v, --verbose` - Enable verbose logging output (shows JSON logs)
- `-h, --help` - Show help for any command
- `-V, --version` - Show version
