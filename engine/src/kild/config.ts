import fs from 'node:fs/promises';
import path from 'node:path';

/** Default model for the Flue layer's helpers. Interactive sessions resolve their
 *  own model (or pi's configured default) via the coding-agent SDK. */
export const DEFAULT_MODEL = process.env.KILD_MODEL ?? 'minimax/MiniMax-M3';

/** kild's state directory: `$KILD_HOME`, else `$XDG_CONFIG_HOME/kild`, else
 *  `~/.config/kild`. Holds projects.json, worktrees, etc. */
export function kildHome(): string {
  if (process.env.KILD_HOME) return process.env.KILD_HOME;
  const base = process.env.XDG_CONFIG_HOME ?? path.join(process.env.HOME ?? '', '.config');
  return path.join(base, 'kild');
}

/**
 * kild config (`.kild/config.json` in a project, and/or `$KILD_HOME/config.json` global).
 * This is how you plug in a framework: a `plugins:` entry points at a dir laid out like a
 * Claude Code plugin (`agents/` + `skills/`), and kild discovers both. kild ships no roles
 * or process of its own — this is where the project brings them.
 */
export interface KildConfig {
  /** Plugin dirs; each contributes its `agents/` and `skills/` subdirs. */
  plugins?: string[];
  /** Extra agent dirs (personas), beyond `.claude/agents` / `.pi/agents`. */
  agentPaths?: string[];
  /** Extra skill dirs, discoverable by every session (driver + all participants). */
  skillPaths?: string[];
  /** Base branch new worktrees are created from and that git status/collisions are
   *  measured against (e.g. `dev`). Overridable per-invocation with `--base`; if unset,
   *  the checkout's current branch is used. */
  baseBranch?: string;
}

export interface ResolvedPluginPaths {
  agentDirs: string[];
  skillDirs: string[];
}

/** Expand a leading `~` to $HOME so config paths can point anywhere on the system. */
function expandHome(p: string): string {
  if (p === '~') return process.env.HOME ?? p;
  if (p.startsWith('~/')) return path.join(process.env.HOME ?? '', p.slice(2));
  return p;
}

async function readConfigFile(file: string): Promise<KildConfig | null> {
  try {
    const parsed = JSON.parse(await fs.readFile(file, 'utf8')) as KildConfig;
    return parsed && typeof parsed === 'object' ? parsed : null;
  } catch {
    return null; // missing or malformed config must never crash discovery
  }
}

/**
 * Resolve agent + skill dirs from the global (`$KILD_HOME/config.json`) and project
 * (`<cwd>/.kild/config.json`) configs. A `plugins:` dir contributes `<dir>/agents` and
 * `<dir>/skills`; `agentPaths`/`skillPaths` add explicit dirs. Every path resolves
 * relative to the config file that declared it. Never throws — bad config yields nothing.
 */
export async function resolvePluginPaths(cwd: string): Promise<ResolvedPluginPaths> {
  const sources = [
    { file: path.join(kildHome(), 'config.json'), base: kildHome() },
    { file: path.join(cwd, '.kild', 'config.json'), base: cwd },
  ];
  const agentDirs: string[] = [];
  const skillDirs: string[] = [];
  for (const { file, base } of sources) {
    const cfg = await readConfigFile(file);
    if (!cfg) continue;
    // Absolute (or `~/…`) paths load from anywhere on the system; relative paths resolve
    // against the config file that declared them.
    for (const plugin of cfg.plugins ?? []) {
      const dir = path.resolve(base, expandHome(plugin));
      agentDirs.push(path.join(dir, 'agents'));
      skillDirs.push(path.join(dir, 'skills'));
    }
    for (const p of cfg.agentPaths ?? []) agentDirs.push(path.resolve(base, expandHome(p)));
    for (const p of cfg.skillPaths ?? []) skillDirs.push(path.resolve(base, expandHome(p)));
  }
  return { agentDirs, skillDirs };
}

/** The configured base branch for `cwd`: project (`<cwd>/.kild/config.json`) over global
 *  (`$KILD_HOME/config.json`). Undefined when neither sets `baseBranch` — the caller then
 *  falls back to the checkout's current branch. Never throws. */
export async function configuredBaseBranch(cwd: string): Promise<string | undefined> {
  const project = await readConfigFile(path.join(cwd, '.kild', 'config.json'));
  if (project?.baseBranch) return project.baseBranch;
  const global = await readConfigFile(path.join(kildHome(), 'config.json'));
  return global?.baseBranch;
}
