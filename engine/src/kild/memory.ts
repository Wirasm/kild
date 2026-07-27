import fs from 'node:fs';
import path from 'node:path';

import { kildHome } from './config.ts';
import { reviewCommits } from './git-review.ts';
import type { Kild } from './kild-types.ts';
import { kildGitStatus } from './worktree-status.ts';

/**
 * Project memory — the filesystem half of "kild remembers".
 *
 * All files live in the project's memory dir (config `memory.dir`, default `.kild/`
 * in the project — callers resolve it via `configuredMemoryDir` and pass it in).
 * Two layers with different owners, deliberately split:
 *
 * - `LOG.md` — append-only, ENGINE-written: one entry per stopped kild, and nothing but
 *   FACTS the engine holds or git can prove — land result, commits vs base, changed
 *   files, each agent's persona/model/spend and pi resume handle, worktree + base.
 *   Free, instant, and incapable of hallucinating because it never summarises: there is
 *   no prose "outcome" line here, and there must never be one. The engine guessing at
 *   what a run *meant* is exactly the failure this ledger was rebuilt to remove.
 * - `MEMORY.md` — the curated, prose half. NOT the engine's: whatever an `hooks.onClose`
 *   hook writes there is the intelligence layer's business. `direction.md` is human-owned
 *   product direction; the engine only ever reads it.
 *
 * Fleet-level memory is `$KILD_HOME/MAIN_MEMORY.md` — the operator's cross-project
 * index. All memory files are personal for now (a project-internal dir is gitignored
 * via its own `.gitignore`; an external dir needs none), which also means worktree
 * checkouts don't carry them: memory is always read from and written to the project's
 * MAIN checkout (`kild.cwd`), never a worktree.
 */

const MEMORY_GITIGNORE = ['MEMORY.md', 'LOG.md', 'direction.md', '.memory-state.json'];
/** Per-file cap for injected memory sections — memory rides every first turn, so it must
 *  stay lean; whoever curates the file keeps it lean at write time, this at read time. */
const SECTION_MAX_CHARS = 6000;

function truncate(text: string, max: number): string {
  const flat = text.trim();
  return flat.length > max ? `${flat.slice(0, max)}…` : flat;
}

function oneLine(text: string, max: number): string {
  return truncate(text.replace(/\s+/g, ' '), max);
}

/** True when `dir` sits inside `projectCwd` (the default `.kild/` case). */
function isInsideProject(projectCwd: string, dir: string): boolean {
  const rel = path.relative(projectCwd, dir);
  return !rel.startsWith('..') && !path.isAbsolute(rel);
}

/** A memory file's path as agents should see it: relative to the project cwd when the
 *  dir is inside the project (the default reads `.kild/LOG.md`, exactly as before),
 *  absolute otherwise. */
function displayPath(projectCwd: string, dir: string, file: string): string {
  const full = path.join(dir, file);
  return isInsideProject(projectCwd, dir) ? path.relative(projectCwd, full) : full;
}

/** Ensure the memory dir exists. A dir inside the project also gets a `.gitignore`
 *  covering the personal memory files (never clobbering an existing one — the
 *  committed-vs-personal call is the user's to change there); an external dir
 *  (e.g. a user-home store) needs no gitignore. */
function ensureMemoryDir(projectCwd: string, dir: string): string {
  fs.mkdirSync(dir, { recursive: true });
  if (isInsideProject(projectCwd, dir)) {
    const gitignore = path.join(dir, '.gitignore');
    if (!fs.existsSync(gitignore)) {
      fs.writeFileSync(gitignore, `${MEMORY_GITIGNORE.join('\n')}\n`);
    }
  }
  return dir;
}

/**
 * The code half of a ledger entry: what git can prove about the kild's work at the
 * moment it stopped. Gathered from the two existing probes — `kildGitStatus`
 * (worktree-status) and `reviewCommits` (git-review) — never re-derived here.
 */
export interface KildLedgerFacts {
  /** Base branch everything below is measured against. */
  base: string;
  /** Branch the kild's dir was on, when git could say. */
  branch?: string;
  /** Commits on the branch that base does not have. */
  commits: number;
  /** Short sha of the newest of those commits — the tip a land would carry over. */
  tip?: string;
  /** Files changed vs base (committed). */
  changedFiles: number;
  /** Files with uncommitted working-tree changes. */
  uncommittedFiles: number;
  /** True only when git can prove the branch's work is already in base: nothing ahead,
   *  base has moved past it, and it is not the base branch itself. Everything else —
   *  including "we cannot tell" — is not landed, because a ledger may not guess.
   *  A recorded {@link landedSha} also makes it true: that is not a guess. */
  landed: boolean;
  /** The merge commit this kild landed as, when the engine performed the land
   *  (`POST /api/kilds/:id/land`). Inference can only ever say "contained in base"; a
   *  recorded sha says WHICH commit, so the ledger prefers it. */
  landedSha?: string;
  /** Any git failure, captured as data (the probes never throw). */
  gitError?: string;
}

/**
 * Collect a kild's git facts for the ledger. `dir` is the kild's effective directory
 * (its worktree if it had one, else its cwd) and `base` its base branch. `landedSha` is the
 * merge the engine recorded when it landed this kild, if it did — a fact it holds outright,
 * so it needs no inference and survives a git failure.
 *
 * Never throws and never blocks a stop: a non-repo, a missing base ref or any git error
 * comes back as `gitError` with zeroed counts, and the entry says so.
 */
export async function collectLedgerFacts(
  dir: string,
  base?: string,
  landedSha?: string,
): Promise<KildLedgerFacts> {
  const status = await kildGitStatus(dir, base);
  const review = await reviewCommits(dir, status.base);
  const gitError = status.error ?? review.error;
  const tip = review.commits[0]?.sha.slice(0, 7);
  return {
    base: status.base,
    branch: status.branch ?? undefined,
    commits: review.commits.length,
    tip,
    changedFiles: status.changedFiles.length,
    uncommittedFiles: status.uncommittedFiles,
    landed:
      landedSha !== undefined ||
      (!gitError &&
        review.commits.length === 0 &&
        status.behind > 0 &&
        status.branch !== null &&
        status.branch !== status.base),
    landedSha,
    gitError,
  };
}

/** The land result line: a merged branch, an unlanded one, or an honest "unknown".
 *  A RECORDED merge sha wins over everything below — it is the one form of "landed" that
 *  can name the commit, and it holds even if git later goes unreadable. */
function landLine(facts: KildLedgerFacts): string {
  if (facts.landedSha) {
    const where = facts.branch ?? 'the working tree';
    return `- landed: yes — ${where} merged into ${facts.base} as ${facts.landedSha.slice(0, 7)}`;
  }
  if (facts.gitError) return `- landed: unknown — git: ${facts.gitError}`;
  // A kild without its own worktree works in the checkout, on the base branch itself:
  // there is no branch to land, and saying "no commits vs main on main" would pretend
  // otherwise.
  if (facts.branch === facts.base) {
    return `- landed: no — ran on ${facts.base} itself, no branch to land`;
  }
  const where = facts.branch ?? 'the working tree';
  if (facts.landed) return `- landed: yes — ${where} is contained in ${facts.base}`;
  if (facts.commits === 0) return `- landed: no — nothing committed on ${where} vs ${facts.base}`;
  return (
    `- landed: no — ${facts.commits} commit${facts.commits === 1 ? '' : 's'} on ${where} ` +
    `not in ${facts.base}${facts.tip ? ` (tip ${facts.tip})` : ''}`
  );
}

/** The code-state line: commits vs base, changed files, and what is still uncommitted. */
function codeLine(facts: KildLedgerFacts): string {
  const uncommitted = facts.uncommittedFiles > 0 ? `, ${facts.uncommittedFiles} uncommitted` : '';
  return (
    `- code: ${facts.commits} commit${facts.commits === 1 ? '' : 's'} vs ${facts.base}, ` +
    `${facts.changedFiles} file${facts.changedFiles === 1 ? '' : 's'} changed${uncommitted}`
  );
}

/** One agent's line: who it was, what it ran on, what it cost, how to reopen it. */
function agentLine(agent: Kild['agents'][number]): string {
  // An attached agent is a harness kild never owned: no persona, no model it can
  // vouch for, and nothing to resume.
  if (agent.ownership === 'attached') return `- attached @${agent.handle}`;
  const persona = agent.persona ?? 'default';
  const model = agent.model ? `, ${agent.model}` : '';
  const spend = [
    agent.tokens === undefined ? '' : `${agent.tokens} tokens`,
    agent.cost === undefined ? '' : `$${agent.cost.toFixed(4)}`,
  ].filter(Boolean);
  const cost = spend.length > 0 ? ` — ${spend.join(', ')}` : '';
  const resume = agent.piSessionFile ?? agent.piSessionId;
  const handle = resume ? ` — pi --session ${resume}` : '';
  return `- agent @${agent.handle} (${persona}${model})${cost}${handle}`;
}

/**
 * One kild's ledger entry — facts only.
 *
 * `goal` is the first message verbatim (what someone asked for, not what the engine
 * thinks happened). There is deliberately NO outcome line: the engine records land
 * result, commits, changed files and per-agent spend, and leaves meaning to whoever
 * curates `MEMORY.md`.
 */
export function formatKildLogEntry(kild: Kild, facts: KildLedgerFacts, closedAt: Date): string {
  const lines: string[] = [];
  lines.push(`## ${closedAt.toISOString().slice(0, 10)} — ${kild.name} (${kild.id})`);
  const kickoff = kild.log[0];
  if (kickoff) lines.push(`- goal: ${oneLine(kickoff.text, 240)}`);
  lines.push(landLine(facts));
  lines.push(codeLine(facts));
  for (const agent of kild.agents) lines.push(agentLine(agent));
  if (kild.worktree) lines.push(`- worktree: kild/${kild.worktree} (base ${kild.base ?? '?'})`);
  return `${lines.join('\n')}\n\n`;
}

/** Append the kild's entry to the memory dir's `LOG.md`; returns the log path.
 *  `dir` is the resolved memory dir (see `configuredMemoryDir`), `facts` the git state
 *  collected at stop (see {@link collectLedgerFacts}). */
export function appendKildLog(
  kild: Kild,
  dir: string,
  facts: KildLedgerFacts,
  closedAt: Date = new Date(),
): string {
  ensureMemoryDir(kild.cwd, dir);
  const logPath = path.join(dir, 'LOG.md');
  fs.appendFileSync(logPath, formatKildLogEntry(kild, facts, closedAt));
  return logPath;
}

function readCapped(file: string): string {
  try {
    const content = fs.readFileSync(file, 'utf8').trim();
    return content ? truncate(content, SECTION_MAX_CHARS) : '';
  } catch {
    return '';
  }
}

/** The project-memory prompt section (curated memory + human-owned direction), read from
 *  the resolved memory dir. Empty string when neither file has content. */
export function projectMemorySection(projectCwd: string, dir: string): string {
  const memory = readCapped(path.join(dir, 'MEMORY.md'));
  const direction = readCapped(path.join(dir, 'direction.md'));
  if (!memory && !direction) return '';
  const memoryPath = displayPath(projectCwd, dir, 'MEMORY.md');
  const directionPath = displayPath(projectCwd, dir, 'direction.md');
  const parts = [
    memory ? `Curated project memory (${memoryPath}):\n${memory}` : '',
    direction ? `Product direction (${directionPath}, human-owned):\n${direction}` : '',
  ].filter(Boolean);
  return `<project-memory>\n${parts.join('\n\n')}\n</project-memory>`;
}

/** Path of the persisted kild transcript the registry writes — a close-event fact, so
 *  a hook can read the run it was fired for. */
export function kildTranscriptPath(kildId: string): string {
  return path.join(kildHome(), 'kilds', `${kildId}.json`);
}
