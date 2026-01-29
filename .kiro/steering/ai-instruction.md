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

1. **Create a new shard** (creates worktree + launches agent):
   ```bash
   shards create <branch> --agent <agent>
   ```

2. **Check what's running**:
   ```bash
   shards list
   ```

3. **Remove shard when done**:
   ```bash
   shards destroy <branch>
   ```

### **Common Commands for AI Agents**

```bash
# Create Kiro CLI shard for bug fixing
shards create bug-fix-123 --agent kiro

# Create Claude shard for feature development
shards create feature-auth --agent claude

# Create Gemini shard for refactoring
shards create refactor-api --agent gemini

# Check all active sessions
shards list

# Clean up finished work
shards destroy bug-fix-123
```

## What Happens When You Create a Shard

1. **Git worktree created** in `~/.shards/worktrees/<project>/<branch>/`
2. **New branch created** with user-specified branch name
3. **Agent launched** in native terminal window in the worktree directory
4. **Session tracked** (persistence planned)

## AI Agent Integration

### **For AI Agents Reading This**

You can use Shards to create isolated workspaces for yourself:

```bash
# Create a new workspace for a specific task
shards create my-task --agent kiro

# This will:
# - Create a new Git worktree
# - Launch a terminal with Kiro CLI
# - Track the session for later cleanup
```

### **Agent-to-Agent Workflow**

```bash
# Agent A creates workspace for Agent B
shards create claude-review --agent claude

# Agent B can later check what's running
shards list

# Agent A can clean up when done
shards destroy claude-review
```

## Best Practices

### **Naming Conventions**
- Use descriptive shard names: `bug-fix-auth`, `feature-payments`, `refactor-db`
- Include issue numbers: `issue-123`, `ticket-456`
- Use agent prefixes: `kiro-debugging`, `claude-testing`

### **Lifecycle Management**
- Always `shards destroy <branch>` when done to clean up worktrees
- Use `shards list` to see what's currently active
- Session persistence and cleanup commands are planned

### **Command Structure**
- Simple commands: `shards create test --agent claude`
- Different agents: `shards create kiro-task --agent kiro`
- Custom branches: `shards create feature-auth --agent gemini`

## Troubleshooting

### **Common Issues**
- **"Not in a Git repository"**: Run shards from within a Git project
- **"Shard already exists"**: Use a different name or stop the existing shard first
- **Terminal doesn't open**: Check if your terminal emulator is supported

### **Recovery Commands**
```bash
# Check what's actually running
shards list

# Clean up when destroy is implemented
shards destroy <branch-name>
```

## Requirements

- Must be run from within a Git repository
- Requires native terminal emulator (Terminal.app, gnome-terminal, etc.)
- Works on macOS, Linux, and Windows

---

**Remember**: Shards is designed for parallel AI development. Use it when you need multiple agents working simultaneously in isolated environments!
