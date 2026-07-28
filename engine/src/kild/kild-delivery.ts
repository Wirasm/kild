import type { Kild, Message } from './kild-types.ts';

/**
 * Delivery — who receives a recorded message, and what the roster learns by trying.
 *
 * Its own slice because it answers its own question. The manager owns the registry, the
 * broadcast and the lifecycle verbs; none of that is needed to work out that `@coder` gets a
 * prompt, `@claude` gets a queued message, and `@ghost` gets a rejection. Everything here
 * takes what it needs as an argument and returns what it found, so the interesting behaviour
 * — a handle with no reader behind it — is testable without a manager, a registry or a kild
 * on disk.
 *
 * **Addressing is the sender's job.** Nothing in this file decides who a message is for: it
 * is handed the recipients the sender named. The only questions it answers are whether those
 * handles exist, whether anything is still behind them, and how each one is reached.
 */

/** How a delivered message reads to the agent receiving it: who, where, what. */
export function formatDelivery(kildName: string, from: string, text: string): string {
  return `[#${kildName}] @${from}: ${text}`;
}

/** The single chokepoint every kild message flows through: one structured trace line so a
 *  whole kild reads back as an ordered log (grep `kild.send`). Kept dead simple — swap
 *  the body for a real logger later without touching call sites. Programmatic tracers
 *  should instead subscribe to the manager (every message is broadcast as a `message`). */
export function traceSend(kildName: string, message: Message): void {
  console.error(
    JSON.stringify({
      t: 'kild.send',
      kild: kildName,
      from: message.from,
      to: message.to,
      chars: message.text.length,
    }),
  );
}

/** Named recipients that are not in this kild's roster. The one addressing question the
 *  engine still answers, and it answers it with "no" — the sender names recipients, so
 *  the only thing left to check is whether those names exist. */
export function unknownRecipients(kild: Kild, to: string[]): string[] {
  const handles = new Set(kild.agents.map((agent) => agent.handle));
  return to.filter((recipient) => !handles.has(recipient));
}

/** Named recipients whose session is gone. A stopped agent STAYS on the roster — its handle
 *  never rebinds and its transcript stays addressable — so membership alone said "yes" to a
 *  send nothing could receive: it validated, `prompt()` returned false into a discarded
 *  return value, and the sender was told "Sent to the kild." A stopped handle is a real
 *  address with no reader, which is a rejection, not a delivery. */
export function stoppedRecipients(kild: Kild, to: string[]): string[] {
  return to.filter((recipient) =>
    kild.agents.some((agent) => agent.handle === recipient && agent.stopped),
  );
}

/** Write a prompt to an owned agent's session. Returns false when there was no session to
 *  write to — which is the whole reason this is a return value and not a void call. */
export type PromptAgent = (agentId: string, text: string, from: string) => boolean;

/**
 * Deliver one recorded message to the recipients its sender named. Returns the handles that
 * could NOT be reached.
 *
 * Two branches, and they differ only in transport — an `owned` agent is a process kild can
 * prompt, an `attached` one is a harness that pulls from its inbox. The single exception is
 * the sender: an agent is never delivered its own message.
 *
 * `prompt()` answers whether the process was there, and that answer used to be discarded: a
 * session that had died on its own (a crash — not a `stop`, so nothing marked the roster)
 * reported a successful send into nothing, and flipped `idle` false on an agent that would
 * never emit `agent_end` to flip it back. Delivery is the moment the engine finds out, so it
 * is the moment the roster learns: an unreachable agent is marked stopped and its idle state
 * is left honest. Persisting and announcing that is the caller's — it owns the disk and the
 * subscribers.
 */
export function deliverMessage(kild: Kild, message: Message, prompt: PromptAgent): string[] {
  const undelivered: string[] = [];
  for (const handle of message.to) {
    if (handle === message.from) continue;
    const agent = kild.agents.find((candidate) => candidate.handle === handle);
    if (!agent) continue;
    if (agent.ownership === 'attached') {
      // `idle` deliberately does NOT flip here — the harness really is idle until it takes
      // its next turn; the drain is what moves it.
      agent.inbox.enqueue({ from: message.from, text: message.text, ts: Date.now() });
      continue;
    }
    if (prompt(agent.id, formatDelivery(kild.name, message.from, message.text), message.from)) {
      // A delivered turn reactivates the agent: it is no longer waiting.
      agent.idle = false;
      continue;
    }
    undelivered.push(agent.handle);
    agent.stopped = true;
    agent.idle = true;
  }
  return undelivered;
}
