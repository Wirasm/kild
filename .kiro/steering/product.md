# Product Overview

## Product Purpose
Shards is a CLI tool that manages multiple AI coding agents (Kiro CLI, Claude Code, Gemini CLI, etc.) running in isolated Git worktrees. It eliminates context switching between scattered terminals by providing centralized management of parallel AI development sessions.

## Target Users
Developers who work with multiple AI coding assistants simultaneously and need to manage parallel development tasks without context switching friction. Designed to be agent-friendly for programmatic use.

## Key Features
- **Isolated Worktrees**: Each shard runs in its own Git worktree with automatic branch creation (`shard_<hash>`)
- **Native Terminal Integration**: Launches AI agents in native terminal windows for seamless interaction
- **Session Management**: Track active shards with persistent registry and status monitoring
- **Cross-Platform Support**: Works on macOS, Linux, and Windows with platform-specific terminal launching
- **Lifecycle Management**: Start, stop, list, cleanup, and inspect shards with comprehensive commands

## Business Objectives
- Reduce context switching overhead when working with multiple AI agents
- Enable parallel AI development workflows without terminal management complexity
- Provide centralized dashboard for AI agent session management
- Support agent-driven workflows where AI assistants can spawn their own shards

## User Journey
1. Developer starts working on a project with an AI agent
2. AI agent or developer runs `shards create <branch> --agent <agent>` to create isolated workspace
3. New Git worktree is created with unique branch, agent launches in native terminal
4. Developer can continue working while agent operates in background
5. Use `shards list` to see all active sessions
6. Clean up with `shards destroy <branch>` when done

## Success Criteria
- Seamless creation and management of isolated AI agent sessions
- Zero context switching between different AI development tasks
- Reliable worktree and session lifecycle management
- Agent-friendly CLI interface for programmatic usage
