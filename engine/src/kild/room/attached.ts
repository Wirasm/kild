/**
 * The attached-participant mailbox: kild's half of the inverted transport.
 *
 * A spawned participant is pushed to (a prompt written to its stdin the moment a post is
 * routed). A participant kild does NOT own — a harness the human drives — cannot be
 * pushed to at all, so it is queued for here and pulls at its own turn boundary.
 *
 * Pure: no I/O, no clock of its own, no knowledge of rooms, sessions, or any harness.
 * The room manager calls it; it never calls back.
 */

/** One unread post waiting for an attached participant. Structured, not pre-formatted:
 *  how it reads is the caller's business (a notice naming the sender, or the body). */
export interface MailboxPost {
  /** Participant name of the sender, or the human handle. */
  from: string;
  /** The post's text, exactly as recorded on the room log. */
  text: string;
  /** Epoch millis, stamped by the caller when the post was recorded. */
  ts: number;
}

/** Consecutive non-empty drains before the mailbox goes quiet for one turn. THE loop
 *  guard: waking an attached harness costs the owner real credits, and two attached
 *  participants replying to each other would otherwise wake each other forever. It lives
 *  here — in the engine — so every harness inherits it and no hook can opt out. */
export const DEFAULT_WAKE_CAP = 3;

/** Hard ceiling on queued posts, so a participant that walked away (its harness closed,
 *  its hook unwired) cannot grow an unbounded queue in a long-lived engine. Oldest are
 *  dropped first — the room log remains the complete record either way. */
export const MAX_QUEUED_POSTS = 50;

/** What one drain reports. */
export interface MailboxDrain {
  /** The posts removed from the queue — empty when there was nothing, or when the wake
   *  cap suppressed this drain. */
  posts: MailboxPost[];
  /** True when this drain reported nothing. THE idle signal: an attached participant is
   *  idle exactly when it asks for mail and there is none to act on — no separate status
   *  verb, no liveness poll. */
  idle: boolean;
  /** True when mail was withheld because the wake cap tripped. The mail stays queued; the
   *  next drain reports it. Callers surface this for observability — it never changes
   *  what the participant is told (it is told nothing, which is the point). */
  capped: boolean;
}

/**
 * An ordered queue of unread posts for one attached participant.
 *
 * Two rules carry the whole design:
 *
 * 1. **Drain is destructive.** Posts are removed as they are reported. A non-destructive
 *    read would re-wake the participant with the same message forever.
 * 2. **Consecutive wakes are capped.** After {@link DEFAULT_WAKE_CAP} non-empty drains in
 *    a row, one drain reports empty regardless of pending mail — which reads to the
 *    caller as "idle", so an autonomous wake loop terminates and can only resume when a
 *    human takes another turn. The counter resets on any drain that reports empty
 *    (including the capped one, so a busy mailbox can never deafen a participant
 *    permanently).
 */
export class Mailbox {
  private readonly queue: MailboxPost[] = [];
  private wakes = 0;

  constructor(private readonly cap: number = DEFAULT_WAKE_CAP) {}

  /** Unread posts currently queued. Observability only — never the delivery path. */
  get pending(): number {
    return this.queue.length;
  }

  /** Non-empty drains since the last one that reported empty. */
  get consecutiveWakes(): number {
    return this.wakes;
  }

  /** Queue one post. Only ever called for a post actually addressed to this participant —
   *  queueing every room post would make the mailbox a firehose and trip the cap on
   *  traffic the participant was never asked to read. */
  enqueue(post: MailboxPost): void {
    this.queue.push(post);
    if (this.queue.length > MAX_QUEUED_POSTS) {
      this.queue.splice(0, this.queue.length - MAX_QUEUED_POSTS);
    }
  }

  /** Destructively read every unread post — atomic by construction (single-threaded
   *  splice), so two drains can never report the same post twice. */
  drain(): MailboxDrain {
    if (this.queue.length === 0) {
      this.wakes = 0;
      return { posts: [], idle: true, capped: false };
    }
    if (this.wakes >= this.cap) {
      // Report empty WITHOUT consuming the mail: the participant is allowed to stop, and
      // whatever is waiting is still waiting when a human next drives a turn.
      this.wakes = 0;
      return { posts: [], idle: true, capped: true };
    }
    const posts = this.queue.splice(0, this.queue.length);
    this.wakes += 1;
    return { posts, idle: false, capped: false };
  }
}
