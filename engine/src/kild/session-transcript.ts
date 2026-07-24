import fs from 'node:fs/promises';

/**
 * Compact, defensive reader for pi session JSONL files (`~/.pi/agent/sessions/…`) —
 * the transcript half of observability: any participant/session with a persisted
 * `piSessionFile` can be read back as a compact conversation, live or archived.
 *
 * The file format is pi's (session header + `message` entries with role
 * user/assistant/toolResult; content blocks of type text/thinking/toolCall). It is
 * an EXTERNAL shape owned by pi, so parsing is best-effort by design: unknown entry
 * types, unknown content kinds, malformed JSON, and partial trailing lines are all
 * skipped — a transcript read must never crash on a file pi is still appending to.
 */

/** One compact transcript entry: who spoke, what they said, which tools they called. */
export interface TranscriptEntry {
  role: 'user' | 'assistant' | 'tool';
  text: string;
  /** Tool names invoked by an assistant entry (arguments deliberately dropped — compact). */
  toolCalls?: string[];
}

export interface SessionTranscript {
  entries: TranscriptEntry[];
  /** Total parseable transcript entries in the file, before the tail cut. */
  total: number;
}

/** Sane default window: the recent conversation, not the whole session. */
export const DEFAULT_TRANSCRIPT_TAIL = 50;
/** Hard cap on requested tail — bounds the response size no matter what a client asks. */
export const MAX_TRANSCRIPT_TAIL = 200;
/** Per-entry text cap — one giant tool result must not blow up the payload. */
const MAX_ENTRY_TEXT = 4_000;

function truncate(text: string): string {
  return text.length > MAX_ENTRY_TEXT ? `${text.slice(0, MAX_ENTRY_TEXT)}… (truncated)` : text;
}

/** Clamp a requested tail to [1, MAX]; undefined/invalid → the default. */
export function clampTranscriptTail(tail?: number): number {
  if (tail === undefined || !Number.isFinite(tail)) return DEFAULT_TRANSCRIPT_TAIL;
  return Math.min(Math.max(Math.floor(tail), 1), MAX_TRANSCRIPT_TAIL);
}

/** One JSONL line → a compact entry, or null for anything unknown/irrelevant. */
function toEntry(line: string): TranscriptEntry | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(line);
  } catch {
    return null; // malformed or partial line (pi may still be writing) — skip
  }
  if (typeof parsed !== 'object' || parsed === null) return null;
  const record = parsed as { type?: unknown; message?: unknown };
  if (record.type !== 'message') return null; // session header, model_change, … — skip
  if (typeof record.message !== 'object' || record.message === null) return null;
  const message = record.message as { role?: unknown; content?: unknown };

  const role =
    message.role === 'user' || message.role === 'assistant'
      ? message.role
      : message.role === 'toolResult'
        ? 'tool'
        : null;
  if (!role) return null; // unknown role — skip

  const texts: string[] = [];
  const toolCalls: string[] = [];
  if (typeof message.content === 'string') {
    texts.push(message.content);
  } else if (Array.isArray(message.content)) {
    for (const block of message.content) {
      if (typeof block !== 'object' || block === null) continue;
      const { type, text, name } = block as { type?: unknown; text?: unknown; name?: unknown };
      if (type === 'text' && typeof text === 'string') texts.push(text);
      else if (type === 'toolCall' && typeof name === 'string') toolCalls.push(name);
      // thinking and any future block kinds: not transcript text — skip
    }
  }

  const text = truncate(texts.join('\n').trim());
  if (!text && toolCalls.length === 0) return null; // e.g. a thinking-only entry
  return { role, text, ...(toolCalls.length > 0 ? { toolCalls } : {}) };
}

/** Parse raw JSONL into a compact transcript, keeping only the last `tail` entries. */
export function parseSessionTranscript(jsonl: string, tail?: number): SessionTranscript {
  const entries: TranscriptEntry[] = [];
  for (const raw of jsonl.split('\n')) {
    const line = raw.trim();
    if (!line) continue;
    const entry = toEntry(line);
    if (entry) entries.push(entry);
  }
  return { entries: entries.slice(-clampTranscriptTail(tail)), total: entries.length };
}

/** Read + parse a pi session file. Throws only on the read itself (missing/unreadable
 *  file) — the caller maps that to its transport error; content problems never throw. */
export async function readSessionTranscript(
  file: string,
  tail?: number,
): Promise<SessionTranscript> {
  return parseSessionTranscript(await fs.readFile(file, 'utf8'), tail);
}
