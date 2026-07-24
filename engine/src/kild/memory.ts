import fs from 'node:fs';
import path from 'node:path';

import { kildHome } from './config.ts';
import type { RoomDecision } from './room/room-decisions.ts';
import { finalNonSystemPost } from './room/room-events.ts';
import type { Room } from './room/room-types.ts';

/**
 * Project memory — the filesystem half of "kild remembers".
 *
 * Two layers with different owners, deliberately split:
 *
 * - `.kild/LOG.md` — append-only, ENGINE-written: one entry per closed room, built
 *   entirely from structured state the engine already holds (goal, outcome, decisions,
 *   agents + their pi resume handles, worktree). Free, instant, never hallucinates.
 * - `.kild/MEMORY.md` — lean CURATED memory, written by an optional synthesis session
 *   (config `memory.synthesis`) that reads the transcript and distills judgment-work:
 *   learnings, direction, the why behind decisions. `.kild/direction.md` is human-owned
 *   product direction; the engine only ever reads it.
 *
 * Fleet-level memory is `$KILD_HOME/MAIN_MEMORY.md` — the operator's cross-project
 * index. All memory files are personal for now (gitignored via `.kild/.gitignore`),
 * which also means worktree checkouts don't carry them: memory is always read from and
 * written to the project's MAIN checkout (`room.cwd`), never a worktree.
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

/** Ensure `.kild/` exists with a `.gitignore` covering the personal memory files.
 *  Never clobbers an existing `.gitignore` (the committed-vs-personal call is the
 *  user's to change there). */
function ensureMemoryDir(projectCwd: string): string {
  const dir = path.join(projectCwd, '.kild');
  fs.mkdirSync(dir, { recursive: true });
  const gitignore = path.join(dir, '.gitignore');
  if (!fs.existsSync(gitignore)) {
    fs.writeFileSync(gitignore, `${MEMORY_GITIGNORE.join('\n')}\n`);
  }
  return dir;
}

function formatDecision(decision: RoomDecision): string {
  const opened = `${decision.key} (${decision.summary}, raised by @${decision.openedBy})`;
  if (decision.resolvedAt === undefined) return `- decision UNRESOLVED at close: ${opened}`;
  const note = decision.note ? `: ${decision.note}` : '';
  return `- decision ${opened} → resolved by @${decision.resolvedBy}${note}`;
}

/** One room's log entry, built purely from engine-held state. */
export function formatRoomLogEntry(room: Room, closedAt: Date): string {
  const lines: string[] = [];
  lines.push(`## ${closedAt.toISOString().slice(0, 10)} — ${room.name} (${room.id})`);
  const kickoff = room.log.find((message) => !message.system);
  if (kickoff) lines.push(`- goal: ${oneLine(kickoff.text, 240)}`);
  lines.push(`- outcome: ${oneLine(finalNonSystemPost(room), 400)}`);
  for (const decision of room.decisions ?? []) lines.push(formatDecision(decision));
  for (const participant of room.participants) {
    const persona = participant.agent ?? 'default';
    const model = participant.model ? `, ${participant.model}` : '';
    const resume = participant.piSessionFile ?? participant.piSessionId;
    const handle = resume ? ` — pi --session ${resume}` : '';
    lines.push(`- agent @${participant.name} (${persona}${model})${handle}`);
  }
  if (room.worktree) lines.push(`- worktree: kild/${room.worktree} (base ${room.base ?? '?'})`);
  return `${lines.join('\n')}\n\n`;
}

/** Append the room's entry to the project's `.kild/LOG.md`; returns the log path. */
export function appendRoomLog(room: Room, closedAt: Date = new Date()): string {
  const dir = ensureMemoryDir(room.cwd);
  const logPath = path.join(dir, 'LOG.md');
  fs.appendFileSync(logPath, formatRoomLogEntry(room, closedAt));
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
 *  the project's MAIN checkout. Empty string when neither file has content. */
export function projectMemorySection(projectCwd: string): string {
  const dir = path.join(projectCwd, '.kild');
  const memory = readCapped(path.join(dir, 'MEMORY.md'));
  const direction = readCapped(path.join(dir, 'direction.md'));
  if (!memory && !direction) return '';
  const parts = [
    memory ? `Curated project memory (.kild/MEMORY.md):\n${memory}` : '',
    direction ? `Product direction (.kild/direction.md, human-owned):\n${direction}` : '',
  ].filter(Boolean);
  return `<project-memory>\n${parts.join('\n\n')}\n</project-memory>`;
}

/** The fleet-memory prompt section (`$KILD_HOME/MAIN_MEMORY.md`) for operator/driver
 *  sessions that steer many projects. Empty string when absent. */
export function fleetMemorySection(): string {
  const memory = readCapped(path.join(kildHome(), 'MAIN_MEMORY.md'));
  if (!memory) return '';
  return `<fleet-memory>\nYour cross-project fleet memory ($KILD_HOME/MAIN_MEMORY.md):\n${memory}\n</fleet-memory>`;
}

/** The synthesis session's task charter — mechanism only (what inputs, what file, what
 *  constraints); its judgment/voice comes from the configured persona, not from here. */
export function synthesisPrompt(room: Room, transcriptPath: string): string {
  return (
    `[kild memory synthesis] Room '${room.name}' just closed in this project.\n\n` +
    `Inputs:\n` +
    `- Room transcript (JSON): ${transcriptPath}\n` +
    `- Engine-written room log: .kild/LOG.md — this room's factual entry is already ` +
    `appended; do not duplicate its facts.\n` +
    `- Current curated memory: .kild/MEMORY.md (may not exist yet)\n` +
    `- Product direction (human-owned, READ-ONLY): .kild/direction.md (may not exist)\n\n` +
    `Task: read the transcript, then update .kild/MEMORY.md so it stays a LEAN curated ` +
    `memory of this project: key decisions and who made them (including resolved ` +
    `needs-decision[...] calls), important human calls, durable learnings, and current ` +
    `direction. Compress and rewrite — do not append-and-grow; keep it under ~120 lines ` +
    `of markdown prose (no schemas, no tables of raw facts the log already holds). ` +
    `Do not modify any other file, do not touch code, do not commit.`
  );
}

/** Path of the persisted room transcript the registry writes (input for synthesis). */
export function roomTranscriptPath(roomId: string): string {
  return path.join(kildHome(), 'rooms', `${roomId}.json`);
}
