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
  - "click on X", "interact with the UI", "press the button"
  - "list elements", "find element", "inspect the UI elements"

  kild-peek captures screenshots of native macOS applications, compares images,
  runs assertions on UI state, and interacts with UI elements. Use it to visually
  verify what applications look like, validate UI state, and automate UI interactions
  during development.

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
kild-peek screenshot [--window <title>] [--app <name>] [--window-id <id>] [--monitor <index>] [--crop <region>] [--wait] [--timeout <ms>] -o <path>
```

Captures a screenshot of a window or monitor.

**Flags:**
- `--window <title>` - Capture window by title (exact match preferred, falls back to partial)
- `--app <name>` - Capture window by app name (can combine with `--window` for precision)
- `--window-id <id>` - Capture window by exact ID
- `--monitor <index>` - Capture specific monitor (0 = primary)
- `--crop <region>` - Crop to region: x,y,width,height (e.g., "0,0,400,50")
- `--wait` - Wait for window to appear (polls until found or timeout)
- `--timeout <ms>` - Timeout in milliseconds when using `--wait` (default: 30000)
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

# Wait for window to appear
kild-peek screenshot --window "Terminal" --wait -o "$SCRATCHPAD/term.png"

# Wait with custom timeout
kild-peek screenshot --window "Terminal" --wait --timeout 5000 -o "$SCRATCHPAD/term.png"

# Capture primary monitor
kild-peek screenshot -o "$SCRATCHPAD/screen.png"

# Capture as JPEG
kild-peek screenshot --window "Terminal" -o "$SCRATCHPAD/term.jpg" --format jpg --quality 90

# Crop to specific region (x,y,width,height)
kild-peek screenshot --app Ghostty --crop 0,0,400,300 -o "$SCRATCHPAD/cropped.png"
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
kild-peek assert [--window <title>] [--app <name>] [--exists|--visible|--similar <baseline>] [--wait] [--timeout <ms>] [--json]
```

Runs assertions on UI state. Returns exit code 0 for pass, 1 for fail.

**Assertion types:**
- `--exists` - Assert window exists
- `--visible` - Assert window is visible (not minimized)
- `--similar <path>` - Assert current screenshot matches baseline image

**Flags:**
- `--window <title>` - Target window by title
- `--app <name>` - Target window by app name (can combine with `--window`)
- `--wait` - Wait for window to appear (polls until found or timeout)
- `--timeout <ms>` - Timeout in milliseconds when using `--wait` (default: 30000)
- `--threshold <0-100>` - Similarity threshold for `--similar` (default: 95)
- `--json` - Output result as JSON

**Examples:**
```bash
# Assert window exists by title
kild-peek assert --window "KILD" --exists

# Assert window exists by app (more reliable)
kild-peek assert --app "KILD" --exists

# Wait for window to appear
kild-peek assert --window "KILD" --exists --wait

# Wait with custom timeout
kild-peek assert --window "KILD" --exists --wait --timeout 5000

# Assert window is visible
kild-peek assert --window "Terminal" --visible

# Precise targeting with app + title
kild-peek assert --app Ghostty --window "Terminal" --visible

# Assert UI matches baseline
kild-peek assert --window "KILD" --similar "$SCRATCHPAD/baseline.png" --threshold 90

# JSON output
kild-peek assert --window "KILD" --exists --json
```

## UI Interaction Commands

### List UI Elements
```bash
kild-peek elements [--window <title>] [--app <name>] [--tree] [--wait] [--timeout <ms>] [--json]
```

Lists all UI elements in a window using the macOS Accessibility API. Shows buttons, text fields, labels, and other interactive elements.

**Flags:**
- `--window <title>` - Target window by title
- `--app <name>` - Target window by app name (can combine with `--window`)
- `--tree` - Display elements as indented tree hierarchy with box-drawing characters
- `--wait` - Wait for window to appear (polls until found or timeout)
- `--timeout <ms>` - Timeout in milliseconds when using `--wait` (default: 30000)
- `--json` - Output as JSON

**Examples:**
```bash
# List all elements in Finder
kild-peek elements --app Finder

# List elements in specific window
kild-peek elements --window "Terminal"

# Display as tree hierarchy
kild-peek elements --app Finder --tree

# Precise targeting with app + window
kild-peek elements --app Ghostty --window "Terminal"

# Wait for window to appear
kild-peek elements --app Finder --wait

# Wait with custom timeout
kild-peek elements --app Finder --wait --timeout 5000

# JSON output for parsing (includes depth field)
kild-peek elements --app KILD --json
```

**Output includes:**
- Element role (button, text field, menu item, etc.)
- Title, value, and description (when available)
- Position (x, y) and size (width, height)
- Enabled state
- Depth in hierarchy (visible in JSON output, or as indentation with `--tree`)

### Find UI Element by Text
```bash
kild-peek find --text <search> [--regex] [--window <title>] [--app <name>] [--wait] [--timeout <ms>] [--json]
```

Finds a UI element by searching for text in its title, value, or description.

**Flags:**
- `--text <search>` - Text to search for (required)
- `--regex` - Treat `--text` value as a regex pattern (case-sensitive by default)
- `--window <title>` - Target window by title
- `--app <name>` - Target window by app name (can combine with `--window`)
- `--wait` - Wait for window to appear (polls until found or timeout)
- `--timeout <ms>` - Timeout in milliseconds when using `--wait` (default: 30000)
- `--json` - Output as JSON

**Examples:**
```bash
# Find "File" menu in Finder (substring match)
kild-peek find --app Finder --text "File"

# Find "Create" button in KILD
kild-peek find --app KILD --text "Create"

# Exact match with regex
kild-peek find --app Finder --text "^File$" --regex

# Alternation pattern (match Create OR Destroy)
kild-peek find --app KILD --text "Create|Destroy" --regex

# Case-insensitive regex (using (?i) prefix)
kild-peek find --app KILD --text "(?i)submit" --regex

# Wait for window to appear
kild-peek find --app KILD --text "Create" --wait

# JSON output
kild-peek find --app Finder --text "Submit" --json
```

**Search behavior:**
- Default: Case-insensitive partial matching (substring)
- With `--regex`: Case-sensitive regex pattern matching (use `(?i)` prefix for case-insensitive)
- Searches title, value, and description fields
- Returns first matching element

### Click UI Element
```bash
kild-peek click [--window <title>] [--app <name>] --at <x,y> [--wait] [--timeout <ms>] [--json]
kild-peek click [--window <title>] [--app <name>] --text <search> [--wait] [--timeout <ms>] [--json]
```

Clicks at specific coordinates or on an element identified by text.

**Coordinate-based click:**
- `--at <x,y>` - Click at coordinates relative to window top-left

**Text-based click:**
- `--text <search>` - Find and click element by text (uses Accessibility API)

**Flags:**
- `--window <title>` - Target window by title
- `--app <name>` - Target window by app name (can combine with `--window`)
- `--wait` - Wait for window to appear (polls until found or timeout)
- `--timeout <ms>` - Timeout in milliseconds when using `--wait` (default: 30000)
- `--json` - Output as JSON

**Note:** `--at` and `--text` are mutually exclusive.

**Examples:**
```bash
# Click at coordinates
kild-peek click --window "Terminal" --at 100,50

# Click by app name
kild-peek click --app Ghostty --at 200,100

# Precise targeting with app + window
kild-peek click --app Ghostty --window "Terminal" --at 150,75

# Click element by text
kild-peek click --app KILD --text "Create"

# Wait for window to appear
kild-peek click --app KILD --text "Create" --wait

# Click button by text
kild-peek click --app Finder --text "Submit"

# JSON output
kild-peek click --app KILD --text "Open" --json
```

### Type Text
```bash
kild-peek type [--window <title>] [--app <name>] <text> [--wait] [--timeout <ms>] [--json]
```

Types text into the focused element in the target window.

**Flags:**
- `--window <title>` - Target window by title
- `--app <name>` - Target window by app name
- `--wait` - Wait for window to appear (polls until found or timeout)
- `--timeout <ms>` - Timeout in milliseconds when using `--wait` (default: 30000)
- `--json` - Output as JSON

**Examples:**
```bash
# Type into Terminal
kild-peek type --window "Terminal" "hello world"

# Type into TextEdit
kild-peek type --app TextEdit "some text"

# Wait for window to appear
kild-peek type --app TextEdit "text" --wait

# JSON output
kild-peek type --window "Terminal" "test" --json
```

### Send Key Combination
```bash
kild-peek key [--window <title>] [--app <name>] <key-combo> [--wait] [--timeout <ms>] [--json]
```

Sends a key or key combination to the target window.

**Supported modifiers:** `cmd`, `shift`, `opt` (or `alt`), `ctrl`

**Common keys:** `enter`, `return`, `tab`, `space`, `escape`, `delete`, `backspace`, arrow keys, function keys

**Flags:**
- `--window <title>` - Target window by title
- `--app <name>` - Target window by app name
- `--wait` - Wait for window to appear (polls until found or timeout)
- `--timeout <ms>` - Timeout in milliseconds when using `--wait` (default: 30000)
- `--json` - Output as JSON

**Examples:**
```bash
# Single key
kild-peek key --window "Terminal" "enter"

# Key combo with modifier
kild-peek key --app Ghostty "cmd+s"

# Multiple modifiers
kild-peek key --window "Terminal" "cmd+shift+p"

# Wait for window to appear
kild-peek key --app Ghostty "enter" --wait

# JSON output
kild-peek key --app TextEdit "tab" --json
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

### Automated UI Interaction

```bash
# 1. List windows to identify target
kild-peek list windows --app KILD

# 2. List UI elements to find clickable items
kild-peek elements --app KILD

# 3. Find specific element
kild-peek find --app KILD --text "Create"

# 4. Click the element by text
kild-peek click --app KILD --text "Create"

# 5. Type into focused field
kild-peek type --app KILD "my-new-branch"

# 6. Send key combination
kild-peek key --app KILD "enter"
```

### Element-Based Testing

```bash
# Check if UI element exists
kild-peek find --app KILD --text "Submit" && echo "Button found"

# Click button and verify outcome
kild-peek click --app KILD --text "Submit"
sleep 1
kild-peek assert --app KILD --visible

# Automated form filling
kild-peek click --app KILD --text "Branch Name"
kild-peek type --app KILD "feature-auth"
kild-peek click --app KILD --text "Create"
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
