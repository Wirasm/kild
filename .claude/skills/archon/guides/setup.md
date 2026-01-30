# Archon Setup Wizard

Interactive setup guide. Follow these steps in order, using AskUserQuestion to gather input.

## Prerequisites

Run these checks first:

```bash
bun --version
git --version
```

If `bun` is not installed, tell the user to install it from https://bun.sh and stop. If `git` is not installed, tell the user to install it and stop.

## Context

The user is inside the **remote-coding-agent** repository — that's how they have access to this skill. The Archon repo path is the current working directory. Store it as `<archon-repo>`.

## Step 1: Setup Mode

Use **AskUserQuestion** to determine setup approach:

```
Header: "Setup mode"
Question: "How would you like to set up Archon?"
Options:
  1. "Quick setup (defaults)" (Recommended) — Use sensible defaults, get running fast
  2. "Guided setup" — Walk through all configuration options interactively
```

Store the choice. If "Quick setup", skip Step 7 (repo config) — defaults will be used. If "Guided setup", include Step 7.

## Step 2: Ask for Target Repo

**IMPORTANT**: The target repo is the user's own project — **never** the remote-coding-agent (Archon) repo itself. Do not suggest or offer the current directory as an option.

Use **AskUserQuestion** to determine the target repository:

```
Header: "Target repo"
Question: "What is the path to the repository you want to work on using Archon? (This should be your own project, not the Archon repo.)"
Options:
  1. "I'll provide a path" (Recommended) — user types or pastes a local repo path
  2. "Clone from GitHub" — user provides a GitHub URL; clone it to ~/.archon/workspaces/
```

When the user selects "I'll provide a path", ask them to type or paste the path. Do not suggest the current working directory.

Store the result as `<target-repo>`.

If "Clone from GitHub": ask for the URL, then:
```bash
archon-repo-path=$(pwd)
mkdir -p ~/.archon/workspaces
cd ~/.archon/workspaces && git clone <url>
```
Set `<target-repo>` to the cloned directory.

## Step 3: Ask for Platforms

Use **AskUserQuestion** with `multiSelect: true`:

```
Header: "Platforms"
Question: "Which platforms do you want to set up? CLI is always included."
Options:
  1. "CLI + GitHub" (Recommended) — CLI for local use, GitHub webhooks for issue/PR automation
  2. "CLI only" — terminal-only, simplest setup
  3. "Telegram" — chat bot via BotFather
  4. "Slack" — Socket Mode app
```

Discord is also available — mention it as the "Other" option text.

## Step 4: Run CLI Setup

**Always run this.** Read and follow `guides/cli.md`:

1. `cd <archon-repo> && bun install`
2. `cd <archon-repo>/packages/cli && bun link`
3. Verify: `archon version`
4. Check Claude is installed: `which claude`, then `claude /login` if needed

## Step 5: Create `.env` (non-CLI platforms only)

**Skip this step if "CLI only" was selected** — CLI uses global Claude auth and SQLite automatically, no `.env` needed.

If any non-CLI platform was selected, create the environment file in the **archon repo root**:

```bash
cd <archon-repo> && cp .env.example .env
```

The `.env` file lives in the archon repo root because that's where the server runs. The CLI also checks this location (and `~/.archon/.env`) for environment variables when invoked.

Tell the user: "I've created `.env` from the template. We'll fill in platform tokens in the next steps."

## Step 6: Run Platform-Specific Setup

Based on the platforms selected in Step 3, read and follow the corresponding guides:

| Selection | Guides to follow |
|-----------|-----------------|
| CLI only | Done — skip to Step 7 |
| CLI + GitHub | `guides/github.md` |
| Telegram | `guides/telegram.md` |
| Slack | `guides/slack.md` |
| Discord (Other) | `guides/discord.md` |

For multiple platforms, follow each platform guide. Each guide will tell the user which values to add to the `.env` created in Step 5.

## Step 7: Configure Target Repo (Guided setup only)

**Skip this step if "Quick setup" was selected in Step 1.** Defaults will be used automatically.

This step creates `.archon/config.yaml` in the target repo. Walk the user through each option using **AskUserQuestion**. Only include non-default values in the generated file.

### 7a: AI Assistant

```
Header: "AI assistant"
Question: "Which AI assistant should Archon use for this repo?"
Options:
  1. "Claude" (Recommended) — Anthropic's Claude Code SDK
  2. "Codex" — OpenAI's Codex SDK
```

Default: `claude`. Only write to config if user picks codex.

### 7b: Worktree Base Branch

```
Header: "Base branch"
Question: "What is the main branch for this repo? (Used as base for worktree creation)"
Options:
  1. "main" (Recommended) — Most common default
  2. "master" — Legacy default
  3. "develop" — Gitflow-style
```

Default: auto-detected from repo. Only write to config if user picks something non-standard.

### 7c: Worktree Copy Files

```
Header: "Worktree files"
Question: "Should Archon copy any files into new worktrees? (Git-ignored files like .env aren't included in worktrees by default)"
Options:
  1. "No extra files needed" (Recommended) — Skip this
  2. "Copy .env" — Copy .env file into worktrees
  3. "Copy .env.example as .env" — Copy .env.example and rename to .env
```

If user selects option 2 or 3, or provides a custom answer, add to `worktree.copyFiles` array. Option 3 uses the `"source -> destination"` syntax: `".env.example -> .env"`.

If user provides "Other", let them list files comma-separated. Each entry becomes an item in `copyFiles`.

### 7d: Default Commands & Workflows Loading

```
Header: "Defaults"
Question: "Archon bundles default commands and workflows. How should they be loaded?"
Options:
  1. "Load at runtime" (Recommended) — Defaults loaded automatically, your repo commands/workflows override by name
  2. "Disable defaults" — Only use commands and workflows defined in your repo's .archon/ folder
```

Default: load at runtime. Only write to config if user disables.

### 7e: Generate config.yaml

Based on the answers, generate `<target-repo>/.archon/config.yaml`. Only include options that differ from defaults. If all defaults were kept, create a minimal commented file.

**Template (all defaults — commented reference):**
```yaml
# Archon Repo Configuration
# All values below show defaults — uncomment to override.

# AI assistant for this repo (claude | codex)
# assistant: claude

# Worktree settings
# worktree:
#   baseBranch: main
#   copyFiles:
#     - ".env"
#     - ".env.example -> .env"

# Default commands/workflows
# defaults:
#   loadDefaultCommands: true
#   loadDefaultWorkflows: true
```

**Template (with user overrides — uncommented values):**
```yaml
# Archon Repo Configuration

assistant: codex

worktree:
  baseBranch: develop
  copyFiles:
    - ".env"

defaults:
  loadDefaultCommands: false
  loadDefaultWorkflows: false
```

Write the file:
```bash
mkdir -p <target-repo>/.archon
# Write config.yaml content
```

## Step 8: Copy Defaults to Target Repo

Copy the bundled default commands and workflows to the target repo so the user can read, inspect, and modify them:

```bash
mkdir -p <target-repo>/.archon/commands
mkdir -p <target-repo>/.archon/workflows
cp -r <archon-repo>/.archon/commands/defaults/* <target-repo>/.archon/commands/
cp -r <archon-repo>/.archon/workflows/defaults/* <target-repo>/.archon/workflows/
```

Tell the user:
- "Copied **{N} commands** and **{M} workflows** to your repo's `.archon/` folder."
- "You can read, modify, or delete any of these. Your repo versions take priority over bundled defaults."
- "Commands are in `.archon/commands/` — these are prompt templates for AI steps."
- "Workflows are in `.archon/workflows/` — these are YAML files defining multi-step AI pipelines."

Count the files copied to fill in {N} and {M}.

## Step 9: Start the Server (non-CLI platforms only)

**Skip if "CLI only" was selected.**

After all platform tokens are in `.env`, read and follow `guides/server.md` to start the server from the archon repo.

## Step 10: Verify from Target Repo

Run a test workflow from the target repo:

```bash
cd <target-repo> && archon workflow list
```

If the CLI is working, also run:

```bash
cd <target-repo> && archon workflow run archon-assist "Say hello"
```

### Troubleshooting

If verification fails:

| Error | Cause | Fix |
|-------|-------|-----|
| `archon: command not found` | CLI not linked | Re-run `cd <archon-repo>/packages/cli && bun link` |
| `Not a git repository` | Not in a git repo | `cd` to the target repo root |
| `No workflows found` | Missing `.archon/workflows/` | Default workflows load automatically — check `archon version` works first |
| Auth errors | Claude not authenticated | Run `claude /login` |

## Step 11: Copy Skill to Target Repo

Copy the archon skill so it's available when the user opens Claude Code in their target repo:

```bash
mkdir -p <target-repo>/.claude/skills
cp -r <archon-repo>/.claude/skills/archon <target-repo>/.claude/skills/archon
```

## Step 12: Final Summary

Tell the user what was set up, then give these instructions:

1. Open a new terminal
2. `cd <target-repo>`
3. Run `claude` to launch Claude Code
4. The archon skill is now loaded — ask Claude to run workflows, fix issues, review PRs, etc.

Example first command in the target repo:
```
"Use archon to fix issue #1"
```

Summarize what was copied to their repo:
- `.archon/commands/` — {N} command templates (editable prompts)
- `.archon/workflows/` — {M} workflow definitions (editable YAML pipelines)
- `.archon/config.yaml` — repo configuration (if guided setup was used)
- `.claude/skills/archon/` — skill for Claude Code integration

The end state: user is in their target repo with the Archon skill available, default commands and workflows copied locally for inspection and customization, using Claude Code as the interface.

## Configuration Reference

For advanced users — these are not needed for basic setup:

- **`.env`** (archon repo root): Platform tokens and secrets. Only needed if running the server for non-CLI platforms. The CLI also checks `$CWD/.env` and `~/.archon/.env` on startup.
- **`~/.archon/config.yaml`** (global): Auto-created with defaults on first run. Controls bot display name, default AI assistant, streaming modes, and concurrency limits. Environment variables in `.env` override matching config.yaml values.
- **`<repo>/.archon/config.yaml`** (per-repo): Controls which AI assistant to use, worktree settings, and default loading behavior. Created by guided setup or manually.

### Repo Config Options Reference

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `assistant` | `claude` \| `codex` | `claude` | AI assistant for this repo |
| `commands.folder` | string | `.archon/commands` | Custom command folder path (relative to repo root) |
| `commands.autoLoad` | boolean | `true` | Auto-load commands on clone |
| `worktree.baseBranch` | string | auto-detected | Base branch for worktree creation |
| `worktree.copyFiles` | string[] | `[]` | Files to copy into new worktrees (supports `"source -> dest"` syntax) |
| `defaults.loadDefaultCommands` | boolean | `true` | Load bundled default commands at runtime |
| `defaults.loadDefaultWorkflows` | boolean | `true` | Load bundled default workflows at runtime |
