import type { Message } from './kild-types.ts';

/**
 * Waiting for somebody else to speak — the wake path a turn-end hook cannot provide.
 *
 * The Stop hook fires when a turn ENDS. A harness sitting idle is therefore unreachable: mail
 * queues, nothing wakes it, and it finds out whenever its operator next happens to prompt.
 * kild cannot close that itself — it does not own an attached harness's process, which is the
 * founding constraint of an attached agent — and no harness hook helps either, because every
 * hook event fires on a turn, tool or session edge and none ticks while a session is idle.
 *
 * So kild owns the *waiting* and the harness owns the *waking*: this blocks until somebody
 * else says something and then exits, and whatever background facility the harness has runs it
 * and reacts to that exit. kild learns nothing about the harness; the harness needs to know
 * nothing about kild's storage.
 *
 * **This reads the message LOG, never the inbox.** The log is non-destructive, so a watcher
 * cannot eat the mail the Stop hook exists to deliver. `watch` is the wake signal; `inbox` is
 * the delivery. Keeping those separate is what makes it safe to run one on a timer.
 *
 * It also means this needs nothing the engine does not already serve — a cursor, which
 * `GET /api/kilds/:id/messages?since=` has always had. Watching answers "has anything new
 * arrived?". It deliberately does NOT answer "is that agent sitting on undrained mail?", which
 * is a question about delivery state and needs surface that does not exist yet. Conflating
 * them would make this depend on machinery it does not need.
 */

/**
 * Timing policy lives HERE, not in `cli.ts`.
 *
 * `cli.ts` calls `dispatch()` at the top of the module, so a `const` declared further down that
 * file is still in its temporal dead zone when the first command reaches it — the failure is
 * `Cannot access 'X' before initialization`, and it takes out the whole verb. An imported
 * module is fully evaluated before the importing module's body runs, so constants here cannot
 * have that problem by construction. That is the class fix; the same bug in its
 * function-declaration form has already been paid for once in this file's neighbour.
 */
/** How often to ask. Loopback and non-destructive, so this is a wake-latency budget rather
 *  than a load one. */
export const WATCH_POLL_MS = 5_000;
/** Consecutive failed polls before the engine counts as gone rather than blipping. A restart
 *  takes a moment and must not be reported as a dead engine. */
export const WATCH_TOLERATED_FAILURES = 3;
/** Default window. Long enough to be useful unattended, short enough that a forgotten watcher
 *  is not immortal. */
export const WATCH_DEFAULT_TIMEOUT_S = 1_800;

/** Floor for a single poll's request timeout. A deliberately tiny `--interval` must not turn a
 *  healthy engine into a failing one just because a loopback round-trip took a few more
 *  milliseconds than the cadence. */
export const WATCH_REQUEST_FLOOR_MS = 1_000;

/**
 * How long ONE poll may wait for an answer.
 *
 * A poll that outlasts its own cadence is not waiting, it is stuck: without a bound the
 * request inherits the client's backstop and a single hung call swallows a whole short window,
 * so a wedged engine looks patient rather than unreachable — the precise distinction the exit
 * codes exist to draw.
 *
 * **`msRemaining` is not optional, and that is the point.** The bound and the loop's deadline
 * are the same question — how much time is left — and answering it in two places is what made
 * `--interval 10 --timeout 3` block ten seconds on its first poll, overrun the requested
 * window threefold, and then report a QUIET engine after every single request had failed.
 * Requiring the caller to pass what remains means the two answers cannot drift, because there
 * is only one.
 */
export function watchRequestTimeout(intervalMs: number, msRemaining: number): number {
  return Math.min(Math.max(intervalMs, WATCH_REQUEST_FLOOR_MS), Math.max(msRemaining, 1));
}

/** What one poll concluded. */
export interface WatchPoll {
  /** Messages from somebody other than the watcher, in arrival order. Empty means keep going. */
  incoming: Message[];
  /** Where to resume from. Advances past the watcher's OWN messages too, so its own sends
   *  are not re-examined on every poll — and can never wake it. */
  cursor: number;
}

/**
 * Decide what a batch of messages means for a watcher.
 *
 * Pure, because the interesting part is the filtering rule and the cursor arithmetic, not the
 * HTTP around it. `handle` is excluded rather than `to` being matched: a watcher wants to know
 * that the conversation moved, and a message addressed to someone else in the same kild is
 * still a reason to look. Only its own voice is not.
 */
export function pollResult(messages: Message[], handle: string, cursor: number): WatchPoll {
  return {
    // `seq <= cursor` is dropped as well as own messages. `since` is documented exclusive, so
    // a message at or behind the cursor should never arrive — but relying on that means one
    // server-side slip re-delivers an already-seen message as fresh mail and wakes a watcher
    // on stale content, forever. The guard is one comparison and it makes this function
    // correct on its own terms rather than on a promise made elsewhere.
    incoming: messages.filter((message) => message.from !== handle && message.seq > cursor),
    cursor: messages.reduce((furthest, message) => Math.max(furthest, message.seq), cursor),
  };
}

/** A one-line summary of who is waiting, for the watcher's stdout. Senders are deduped and
 *  ordered by first appearance, so the line reads like the conversation did. */
export function watchSummary(incoming: Message[]): string {
  const senders: string[] = [];
  for (const message of incoming) if (!senders.includes(message.from)) senders.push(message.from);
  const who = senders.map((sender) => `@${sender}`).join(', ');
  const count = incoming.length;
  return `${count} new message${count === 1 ? '' : 's'} from ${who}`;
}

/**
 * Exit codes, which are the whole interface for whatever runs this in the background.
 *
 * A quiet engine and a DEAD engine must never look alike. If both exited 0, every harness built
 * on `watch` would inherit the failure this command exists to remove: a wake mechanism that
 * goes silent and is indistinguishable from one that simply had nothing to report. That is not
 * retrofittable once harnesses depend on the behaviour, so it is decided here, first.
 */
export const WATCH_EXIT = {
  /** Somebody spoke. The summary is on stdout. */
  mail: 0,
  /** Usage, or no attachment to resolve — an ordinary loud CLI failure. */
  usage: 1,
  /** The window elapsed with nothing new. The engine was reachable throughout; it was quiet.
   *  A caller that wants to keep waiting simply runs it again. */
  quiet: 2,
  /** The engine stopped answering and did not come back. Distinct from `quiet` because the
   *  answer is "I no longer know", not "nothing happened". */
  unreachable: 3,
} as const;
