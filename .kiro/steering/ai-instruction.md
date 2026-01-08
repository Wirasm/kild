# Shards CLI - AI Agent Instructions

## What is Shards?

Shards is a CLI tool that manages multiple AI coding agents in isolated Git worktrees. Think of it as "browser tabs for AI agents" - each shard runs in its own workspace without interfering with others.

## When to Use Shards

### ✅ **Perfect Use Cases**
- **Parallel development**: Work on multiple features/bugs simultaneously with different AI agents
- **Context isolation**: Prevent agents from conflicting over the same working directory
- **Background tasks**: Let an AI agent work on a side issue while you focus on something else
- **Agent collaboration**: Multiple AI agents working on different parts of the same project
- **Experimentation**: Try different approaches in isolated environments

### ❌ **Not Suitable For**
- Single-threaded development (just use your main branch)
- Non-Git projects (Shards requires a Git repository)
- Projects where you need agents to share the same working directory

## How to Use Shards

### **Basic Workflow**

1. **Start a new shard** (creates worktree + launches agent):
   ```bash
   shards start <shard-name> <agent-command>
   ```

2. **Check what's running**:
   ```bash
   shards list
   ```

3. **Get details about a shard**:
   ```bash
   shards info <shard-name>
   ```

4. **Stop and cleanup when done**:
   ```bash
   shards stop <shard-name>
   ```

### **Common Commands for AI Agents**

```bash
# Start Kiro CLI for bug fixing
shards start bug-fix-123 "kiro-cli chat"

# Start Claude for feature development
shards start feature-auth "claude-code"

# Start Gemini for refactoring
shards start refactor-api "gemini-cli"

# Check all active sessions
shards list

# Clean up finished work
shards stop bug-fix-123
```

## What Happens When You Start a Shard

1. **Git worktree created** in `.shards/<shard-name>/`
2. **New branch created** with format `shard_<uuid>`
3. **Agent launched** in native terminal window in the worktree directory
4. **Session tracked** in `~/.shards/registry.json`

## AI Agent Integration

### **For AI Agents Reading This**

You can use Shards to create isolated workspaces for yourself:

```bash
# Create a new workspace for a specific task
shards start my-task "kiro-cli chat"

# This will:
# - Create a new Git worktree
# - Launch a terminal with Kiro CLI
# - Track the session for later cleanup
```

### **Agent-to-Agent Workflow**

```bash
# Agent A creates workspace for Agent B
shards start claude-review "claude-code --review"

# Agent B can later check what's running
shards list

# Agent A can clean up when done
shards stop claude-review
```

## Best Practices

### **Naming Conventions**
- Use descriptive shard names: `bug-fix-auth`, `feature-payments`, `refactor-db`
- Include issue numbers: `issue-123`, `ticket-456`
- Use agent prefixes: `kiro-debugging`, `claude-testing`

### **Lifecycle Management**
- Always `shards stop <name>` when done to clean up worktrees
- Use `shards cleanup` periodically to remove orphaned sessions
- Use `shards list` to see what's currently active

### **Command Structure**
- Simple commands: `shards start test "echo hello"`
- Complex commands: `shards start kiro "kiro-cli chat --model gpt-4"`
- Commands with flags: `shards start debug "node --inspect app.js"`

## Troubleshooting

### **Common Issues**
- **"Not in a Git repository"**: Run shards from within a Git project
- **"Shard already exists"**: Use a different name or stop the existing shard first
- **Terminal doesn't open**: Check if your terminal emulator is supported

### **Recovery Commands**
```bash
# Clean up all orphaned sessions
shards cleanup

# Check what's actually running
shards list

# Get detailed info about a problematic shard
shards info <shard-name>
```

## Requirements

- Must be run from within a Git repository
- Requires native terminal emulator (Terminal.app, gnome-terminal, etc.)
- Works on macOS, Linux, and Windows

---

**Remember**: Shards is designed for parallel AI development. Use it when you need multiple agents working simultaneously in isolated environments!
