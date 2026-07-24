import type { Room, RoomMessage } from './room-types.ts';

/**
 * Keyed decisions — the durable "this needs a call" ledger on a room.
 *
 * The invariant (borrowed from firstmate's status fold): a raised decision can never
 * close *implicitly*. A `needs-decision[key]:` line in any ordinary post opens a keyed
 * decision on the room; only an explicit `resolved[key]` line closes it. A later "done"
 * post, an idle room, or a dropped gate digest cannot mask it — `rooms_status` keeps
 * surfacing it and `close_room` refuses while any decision is open (operator `force`
 * is the escape hatch).
 *
 * This module is the ONE place that decides what counts as a decision marker — the
 * same pattern as `parseMentions` for addressing. Grammar, line-anchored inside a
 * post's text (a post may carry prose around the marker lines):
 *
 *   needs-decision[<key>]: <summary>   — opens (or refreshes) decision <key>
 *   resolved[<key>]                    — closes it; optional `: <note>`
 *
 * Keys are `[A-Za-z0-9._-]+`. Who may resolve is deliberately unrestricted: resolution
 * is attributed and on the log, so it is loud — identity *enforcement* is the
 * engine-derived-actors slice's job, not this one's.
 */
export interface RoomDecision {
  key: string;
  summary: string;
  /** Participant name or HUMAN — whoever posted the opening marker. */
  openedBy: string;
  /** Epoch millis of the opening post. */
  openedAt: number;
  resolvedBy?: string;
  resolvedAt?: number;
  note?: string;
}

const OPEN_RE = /^\s*needs-decision\[([A-Za-z0-9._-]+)\]:\s*(\S.*)$/;
const RESOLVE_RE = /^\s*resolved\[([A-Za-z0-9._-]+)\](?::\s*(.*\S))?\s*$/;

export function openDecisions(room: Pick<Room, 'decisions'>): RoomDecision[] {
  return (room.decisions ?? []).filter((decision) => decision.resolvedAt === undefined);
}

/** One-line human-readable list of a room's open decisions (empty string when none). */
export function formatOpenDecisions(room: Pick<Room, 'decisions'>): string {
  return openDecisions(room)
    .map((decision) => `${decision.key} (${decision.summary}, raised by @${decision.openedBy})`)
    .join('; ');
}

/**
 * Fold one post's decision markers into the room's ledger (mutates `room.decisions`).
 * Re-opening a key that is already open refreshes its summary; resolving a key with no
 * open decision is a silent no-op (the marker still sits attributed on the log).
 * Returns whether the ledger changed, so the caller knows to re-persist.
 */
export function applyDecisionMarkers(
  room: Pick<Room, 'decisions'>,
  message: Pick<RoomMessage, 'from' | 'text' | 'ts' | 'system'>,
): boolean {
  if (message.system) return false;
  let changed = false;
  for (const line of message.text.split('\n')) {
    const open = OPEN_RE.exec(line);
    if (open) {
      const [, key, summary] = open as unknown as [string, string, string];
      const existing = openDecisions(room).find((decision) => decision.key === key);
      if (existing) {
        existing.summary = summary;
      } else {
        room.decisions = room.decisions ?? [];
        room.decisions.push({ key, summary, openedBy: message.from, openedAt: message.ts });
      }
      changed = true;
      continue;
    }
    const resolve = RESOLVE_RE.exec(line);
    if (resolve) {
      const [, key, note] = resolve as unknown as [string, string, string | undefined];
      const existing = openDecisions(room).find((decision) => decision.key === key);
      if (!existing) continue;
      existing.resolvedBy = message.from;
      existing.resolvedAt = message.ts;
      if (note) existing.note = note;
      changed = true;
    }
  }
  return changed;
}
