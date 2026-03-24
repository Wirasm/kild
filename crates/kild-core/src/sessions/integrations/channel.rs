//! MCP channel server integration for fleet communication.
//!
//! Installs the `kild-fleet` channel server at `~/.kild/channels/fleet/` and
//! patches `.mcp.json` in worktrees to register it. The channel server watches
//! inbox files and pushes notifications into Claude Code sessions in real-time.
//!
//! The file-based inbox protocol remains the source of truth — the channel is
//! a notification and tooling layer on top.

use std::path::Path;

use kild_config::KildConfig;
use kild_paths::KildPaths;
use tracing::{debug, info, warn};

use crate::sessions::fleet;

/// Embedded channel server source (TypeScript/Bun).
const CHANNEL_SERVER: &str = r#"#!/usr/bin/env bun
import { Server } from '@modelcontextprotocol/sdk/server/index.js'
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js'
import { ListToolsRequestSchema, CallToolRequestSchema } from '@modelcontextprotocol/sdk/types.js'
import { watch } from 'fs'
import { readFileSync, writeFileSync, readdirSync, existsSync, statSync } from 'fs'
import { join, basename, dirname } from 'path'

const INBOX = process.env.KILD_INBOX
const FLEET_DIR = process.env.KILD_FLEET_DIR
const BRANCH = process.env.KILD_SESSION_BRANCH || 'unknown'
const IS_BRAIN = !!FLEET_DIR

if (!INBOX && !FLEET_DIR) {
  // Not a fleet session — exit silently. MCP server start will fail gracefully.
  process.exit(0)
}

const server = new Server(
  { name: 'kild-fleet', version: '0.1.0' },
  {
    capabilities: { experimental: { 'claude/channel': {} }, tools: {} },
    instructions: IS_BRAIN
      ? [
          'Fleet events arrive as <channel source="kild-fleet" branch="..." event="...">.',
          'You are the Honryū brain. React to worker status changes and reports.',
          'Use send_to_worker to assign tasks. Use list_fleet to see all workers.',
        ].join(' ')
      : [
          'Fleet events arrive as <channel source="kild-fleet" event="task">.',
          `You are worker "${BRANCH}". When you receive a task, use report_status("working") to acknowledge,`,
          'then execute it. When done, use report_status("done", "your report here").',
          'Use send_to_brain to message the brain supervisor.',
        ].join(' '),
  },
)

// --- Tools ---

const WORKER_TOOLS = [
  {
    name: 'report_status',
    description: 'Update your fleet status and optionally write a completion report.',
    inputSchema: {
      type: 'object' as const,
      properties: {
        status: { type: 'string', enum: ['idle', 'working', 'done', 'blocked'], description: 'Your current status' },
        report: { type: 'string', description: 'Task report content (written to report.md). Include when status is done.' },
      },
      required: ['status'],
    },
  },
  {
    name: 'send_to_brain',
    description: 'Send a message to the brain supervisor (writes to brain inbox).',
    inputSchema: {
      type: 'object' as const,
      properties: {
        text: { type: 'string', description: 'Message text' },
      },
      required: ['text'],
    },
  },
]

const BRAIN_TOOLS = [
  {
    name: 'report_status',
    description: 'Update your fleet status.',
    inputSchema: {
      type: 'object' as const,
      properties: {
        status: { type: 'string', enum: ['idle', 'working', 'done', 'blocked'] },
      },
      required: ['status'],
    },
  },
  {
    name: 'send_to_worker',
    description: 'Send a task or message to a fleet worker (writes task.md to their inbox).',
    inputSchema: {
      type: 'object' as const,
      properties: {
        branch: { type: 'string', description: 'Worker branch name' },
        text: { type: 'string', description: 'Task or message text' },
      },
      required: ['branch', 'text'],
    },
  },
]

const COMMON_TOOLS = [
  {
    name: 'list_fleet',
    description: 'List all fleet members with their current inbox status.',
    inputSchema: { type: 'object' as const, properties: {} },
  },
]

server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: [...(IS_BRAIN ? BRAIN_TOOLS : WORKER_TOOLS), ...COMMON_TOOLS],
}))

server.setRequestHandler(CallToolRequestSchema, async (req) => {
  const { name, arguments: args } = req.params

  switch (name) {
    case 'report_status': {
      if (!INBOX) return { content: [{ type: 'text', text: 'No KILD_INBOX set' }] }
      writeFileSync(join(INBOX, 'status'), args.status)
      if (args.report) {
        writeFileSync(join(INBOX, 'report.md'), args.report)
      }
      return { content: [{ type: 'text', text: `Status updated to ${args.status}` }] }
    }

    case 'send_to_brain': {
      if (!FLEET_DIR && !INBOX) return { content: [{ type: 'text', text: 'No fleet directory available' }] }
      // Worker: FLEET_DIR is not set, but we can derive brain inbox from INBOX path
      // INBOX = ~/.kild/inbox/<project>/worker-branch, brain = ~/.kild/inbox/<project>/honryu
      const projectDir = dirname(INBOX!)
      const brainInbox = join(projectDir, 'honryu')
      if (!existsSync(brainInbox)) return { content: [{ type: 'text', text: 'Brain inbox not found — is honryu running?' }] }
      const tmpPath = join(brainInbox, '.task.md.tmp')
      writeFileSync(tmpPath, args.text)
      const { renameSync } = await import('fs')
      renameSync(tmpPath, join(brainInbox, 'task.md'))
      return { content: [{ type: 'text', text: `Message sent to brain` }] }
    }

    case 'send_to_worker': {
      if (!FLEET_DIR) return { content: [{ type: 'text', text: 'Only the brain can send to workers' }] }
      const safeBranch = args.branch.replace(/\//g, '_')
      const workerInbox = join(FLEET_DIR, safeBranch)
      if (!existsSync(workerInbox)) return { content: [{ type: 'text', text: `Worker inbox not found for '${args.branch}'` }] }
      const tmpPath = join(workerInbox, '.task.md.tmp')
      writeFileSync(tmpPath, args.text)
      const { renameSync } = await import('fs')
      renameSync(tmpPath, join(workerInbox, 'task.md'))
      return { content: [{ type: 'text', text: `Task sent to ${args.branch}` }] }
    }

    case 'list_fleet': {
      const dir = FLEET_DIR || (INBOX ? dirname(INBOX) : null)
      if (!dir || !existsSync(dir)) return { content: [{ type: 'text', text: 'No fleet directory found' }] }
      const entries = readdirSync(dir)
        .filter(f => { try { return statSync(join(dir, f)).isDirectory() } catch { return false } })
        .map(branch => {
          const statusPath = join(dir, branch, 'status')
          const status = existsSync(statusPath) ? readFileSync(statusPath, 'utf8').trim() : 'unknown'
          const hasTask = existsSync(join(dir, branch, 'task.md'))
          const hasReport = existsSync(join(dir, branch, 'report.md'))
          return `${branch}: ${status}${hasTask ? ' [task]' : ''}${hasReport ? ' [report]' : ''}`
        })
      return { content: [{ type: 'text', text: entries.length ? entries.join('\n') : 'No fleet members found' }] }
    }

    default:
      return { content: [{ type: 'text', text: `Unknown tool: ${name}` }] }
  }
})

// --- Connect ---
await server.connect(new StdioServerTransport())

// --- File watching ---

let debounce = new Map<string, ReturnType<typeof setTimeout>>()

function pushNotification(branch: string, event: string, content: string) {
  server.notification({
    method: 'notifications/claude/channel',
    params: {
      content,
      meta: { branch, event, ts: new Date().toISOString() },
    },
  }).catch(() => {}) // best-effort
}

function onFileChange(dir: string, filename: string | null) {
  if (!filename || filename.startsWith('.')) return
  const name = basename(filename)
  if (!['task.md', 'status', 'report.md'].includes(name)) return

  // Debounce per file (100ms) — fs.watch can fire multiple events for one write
  const key = join(dir, filename)
  const existing = debounce.get(key)
  if (existing) clearTimeout(existing)
  debounce.set(key, setTimeout(() => {
    debounce.delete(key)
    try {
      const filePath = join(dir, filename!)
      if (!existsSync(filePath)) return
      const content = readFileSync(filePath, 'utf8')
      // For brain watching fleet dir: filename is "worker-branch/status"
      // For worker watching own inbox: filename is "status"
      const parts = filename!.split('/')
      const branch = parts.length > 1 ? parts[0] : BRANCH
      const event = name.replace('.md', '')
      pushNotification(branch, event, content)
    } catch {}
  }, 100))
}

if (IS_BRAIN && FLEET_DIR && existsSync(FLEET_DIR)) {
  watch(FLEET_DIR, { recursive: true }, (_, filename) => onFileChange(FLEET_DIR!, filename))
} else if (INBOX && existsSync(INBOX)) {
  watch(INBOX, {}, (_, filename) => onFileChange(INBOX!, filename))
}
"#;

/// Embedded package.json for the channel server.
const CHANNEL_PACKAGE_JSON: &str = r#"{
  "name": "kild-fleet-channel",
  "private": true,
  "type": "module",
  "dependencies": {
    "@modelcontextprotocol/sdk": "^1.12.0"
  }
}
"#;

/// Ensure the kild-fleet channel server is installed at `~/.kild/channels/fleet/`.
///
/// Always overwrites `server.ts` to pick up updates. Only writes `package.json`
/// if missing (preserving installed node_modules).
pub fn ensure_channel_server_installed(paths: &KildPaths) -> Result<(), String> {
    let fleet_dir = paths.fleet_channel_dir();

    std::fs::create_dir_all(&fleet_dir)
        .map_err(|e| format!("failed to create {}: {}", fleet_dir.display(), e))?;

    // Always overwrite server.ts (same pattern as claude-status hook).
    let server_path = fleet_dir.join("server.ts");
    std::fs::write(&server_path, CHANNEL_SERVER)
        .map_err(|e| format!("failed to write {}: {}", server_path.display(), e))?;

    // Only write package.json if missing (don't clobber installed deps).
    let package_path = fleet_dir.join("package.json");
    if !package_path.exists() {
        std::fs::write(&package_path, CHANNEL_PACKAGE_JSON)
            .map_err(|e| format!("failed to write {}: {}", package_path.display(), e))?;
    }

    info!(
        event = "core.fleet.channel_server_installed",
        path = %fleet_dir.display(),
    );

    Ok(())
}

/// Ensure `.mcp.json` in the worktree contains the `kild-fleet` channel server entry.
///
/// Idempotent — skips if the entry already exists.
pub fn ensure_mcp_json(worktree_path: &Path, paths: &KildPaths) -> Result<(), String> {
    let mcp_path = worktree_path.join(".mcp.json");
    let server_ts = paths.fleet_channel_dir().join("server.ts");
    let server_ts_str = server_ts.display().to_string();

    let mut config: serde_json::Value = if mcp_path.exists() {
        let content = std::fs::read_to_string(&mcp_path)
            .map_err(|e| format!("failed to read {}: {}", mcp_path.display(), e))?;
        serde_json::from_str(&content).map_err(|e| {
            format!(
                "failed to parse {}: {} — fix JSON syntax or remove the file to reset",
                mcp_path.display(),
                e
            )
        })?
    } else {
        serde_json::json!({})
    };

    let servers = config
        .as_object_mut()
        .ok_or(".mcp.json root is not an object")?
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));

    let servers_obj = servers
        .as_object_mut()
        .ok_or("\"mcpServers\" field in .mcp.json is not an object")?;

    if servers_obj.contains_key("kild-fleet") {
        debug!(event = "core.fleet.mcp_json_already_configured");
        return Ok(());
    }

    servers_obj.insert(
        "kild-fleet".to_string(),
        serde_json::json!({
            "command": "bun",
            "args": [server_ts_str]
        }),
    );

    let content = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("failed to serialize .mcp.json: {}", e))?;

    std::fs::write(&mcp_path, format!("{}\n", content))
        .map_err(|e| format!("failed to write {}: {}", mcp_path.display(), e))?;

    info!(
        event = "core.fleet.mcp_json_patched",
        path = %mcp_path.display(),
    );

    Ok(())
}

/// Remove the `kild-fleet` entry from `.mcp.json` in the worktree.
///
/// Best-effort — for `--main` sessions where the worktree is the project root.
/// Deletes the file entirely if it only contained the kild-fleet entry.
pub fn cleanup_mcp_json(worktree_path: &Path) {
    let mcp_path = worktree_path.join(".mcp.json");
    if !mcp_path.exists() {
        return;
    }

    let content = match std::fs::read_to_string(&mcp_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let mut config: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return,
    };

    let removed = config
        .get_mut("mcpServers")
        .and_then(|s| s.as_object_mut())
        .map(|servers| servers.remove("kild-fleet").is_some())
        .unwrap_or(false);

    if !removed {
        return;
    }

    // If mcpServers is now empty, remove the file entirely.
    let is_empty = config
        .get("mcpServers")
        .and_then(|s| s.as_object())
        .is_some_and(|s| s.is_empty());

    if is_empty {
        let _ = std::fs::remove_file(&mcp_path);
        info!(
            event = "core.fleet.mcp_json_removed",
            path = %mcp_path.display(),
        );
    } else {
        if let Ok(json) = serde_json::to_string_pretty(&config) {
            let _ = std::fs::write(&mcp_path, format!("{}\n", json));
        }
        info!(
            event = "core.fleet.mcp_json_entry_removed",
            path = %mcp_path.display(),
        );
    }
}

/// Check if `bun` is available in PATH.
fn bun_available() -> bool {
    std::process::Command::new("bun")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Set up the MCP channel integration for a fleet session.
///
/// Best-effort — warns on failure but never blocks session creation.
/// No-op when: fleet mode inactive, channels disabled in config, agent is not
/// claude, or Bun is not installed.
pub(crate) fn setup_channel_integration(
    agent: &str,
    worktree_path: &Path,
    branch: &str,
    _is_main_worktree: bool,
    config: &KildConfig,
) {
    // Guard: only for claude agents in fleet mode with channels enabled.
    if agent != "claude" {
        return;
    }
    if !fleet::fleet_mode_active(branch) {
        return;
    }
    if !config.fleet.channels() {
        return;
    }
    if !bun_available() {
        warn!(event = "core.fleet.channel.bun_not_found");
        eprintln!(
            "Warning: Bun not found — fleet channel server requires Bun. \
             Install from https://bun.sh or run `kild init-channels` after installing."
        );
        return;
    }

    let paths = match KildPaths::resolve() {
        Ok(p) => p,
        Err(e) => {
            warn!(event = "core.fleet.channel.paths_failed", error = %e);
            return;
        }
    };

    if let Err(e) = ensure_channel_server_installed(&paths) {
        warn!(event = "core.fleet.channel.install_failed", error = %e);
        eprintln!("Warning: Failed to install fleet channel server: {e}");
        return;
    }

    if let Err(e) = ensure_mcp_json(worktree_path, &paths) {
        warn!(event = "core.fleet.channel.mcp_json_failed", error = %e);
        eprintln!("Warning: Failed to configure .mcp.json: {e}");
    }
}
