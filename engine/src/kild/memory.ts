import fs from 'node:fs';
import path from 'node:path';

import { kildHome } from './config.ts';
import type { Kild } from './kild-types.ts';

/**
 * Project memory — the filesystem half of "kild remembers".
 *
 * All files live in the project's memory dir (config `memory.dir`, default `.kild/`
 * in the project — callers resolve it via `configuredMemoryDir` and pass it in).
 * Two layers with different owners, deliberately split:
 *
 * - `LOG.md` — append-only, ENGINE-written: one entry per stopped kild, built
 *   entirely from structured state the engine already holds (goal, outcome, agents +
 *   their pi resume handles, worktree). Free, instant, never hallucinates.
 * - `MEMORY.md` — lean CURATED memory, written by an optional synthesis session
 *   (config `memory.synthesis`) that reads the transcript and distills judgment-work:
 *   learnings, direction, the why behind decisions. `direction.md` is human-owned
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
 *  stay lean; the synthesis charter enforces leanness at write time, this at read time. */
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

/** Stable fallback when a kild produced no messages at all. */
export const NO_FINAL_MESSAGE = '(no messages recorded)';

/**
 * The ledger's `outcome:` source: the text of the kild's last message.
 *
 * A heuristic, and knowingly so — it stands in for facts the engine already holds (land
 * result, commits vs base, changed files) and is replaced by them in a later slice. It
 * lives here because the ledger is its only consumer.
 */
function finalMessageText(kild: Pick<Kild, 'log'>): string {
  return kild.log.at(-1)?.text ?? NO_FINAL_MESSAGE;
}

/** One kild's log entry, built purely from engine-held state. */
export function formatKildLogEntry(kild: Kild, closedAt: Date): string {
  const lines: string[] = [];
  lines.push(`## ${closedAt.toISOString().slice(0, 10)} — ${kild.name} (${kild.id})`);
  const kickoff = kild.log[0];
  if (kickoff) lines.push(`- goal: ${oneLine(kickoff.text, 240)}`);
  lines.push(`- outcome: ${oneLine(finalMessageText(kild), 400)}`);
  for (const agent of kild.agents) {
    // An attached agent is a harness kild never owned: no persona, no model it can
    // vouch for, and nothing to resume.
    if (agent.ownership === 'attached') {
      lines.push(`- attached @${agent.handle}`);
      continue;
    }
    const persona = agent.persona ?? 'default';
    const model = agent.model ? `, ${agent.model}` : '';
    const resume = agent.piSessionFile ?? agent.piSessionId;
    const handle = resume ? ` — pi --session ${resume}` : '';
    lines.push(`- agent @${agent.handle} (${persona}${model})${handle}`);
  }
  if (kild.worktree) lines.push(`- worktree: kild/${kild.worktree} (base ${kild.base ?? '?'})`);
  return `${lines.join('\n')}\n\n`;
}

/** Append the kild's entry to the memory dir's `LOG.md`; returns the log path.
 *  `dir` is the resolved memory dir (see `configuredMemoryDir`). */
export function appendKildLog(kild: Kild, dir: string, closedAt: Date = new Date()): string {
  ensureMemoryDir(kild.cwd, dir);
  const logPath = path.join(dir, 'LOG.md');
  fs.appendFileSync(logPath, formatKildLogEntry(kild, closedAt));
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

/** The synthesis session's task charter — mechanism only (what inputs, what file, what
 *  constraints); its judgment/voice comes from the configured persona, not from here.
 *  `dir` is the resolved memory dir — the charter must name the ACTUAL paths, or the
 *  synthesis agent writes to the wrong place. */
export function synthesisPrompt(kild: Kild, transcriptPath: string, dir: string): string {
  const logPath = displayPath(kild.cwd, dir, 'LOG.md');
  const memoryPath = displayPath(kild.cwd, dir, 'MEMORY.md');
  const directionPath = displayPath(kild.cwd, dir, 'direction.md');
  return (
    `[kild memory synthesis] Kild '${kild.name}' just stopped in this project.\n\n` +
    `Inputs:\n` +
    `- Kild transcript (JSON): ${transcriptPath}\n` +
    `- Engine-written kild log: ${logPath} — this kild's factual entry is already ` +
    `appended; do not duplicate its facts.\n` +
    `- Current curated memory: ${memoryPath} (may not exist yet)\n` +
    `- Product direction (human-owned, READ-ONLY): ${directionPath} (may not exist)\n\n` +
    `Task: read the transcript, then update ${memoryPath} so it stays a LEAN curated ` +
    `memory of this project: key decisions and who made them (including resolved ` +
    `needs-decision[...] calls), important human calls, durable learnings, and current ` +
    `direction. Compress and rewrite — do not append-and-grow; keep it under ~120 lines ` +
    `of markdown prose (no schemas, no tables of raw facts the log already holds). ` +
    `Do not modify any other file, do not touch code, do not commit.`
  );
}

/** Path of the persisted kild transcript the registry writes (input for synthesis). */
export function kildTranscriptPath(kildId: string): string {
  return path.join(kildHome(), 'kilds', `${kildId}.json`);
}
