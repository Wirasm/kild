import { type AttachedAgent, HUMAN, type Kild, type Message } from './kild-types.ts';

/** The side-effects routing needs, injected so the router stays decoupled from the
 *  AgentManager and the WS layer (and unit-testable on its own). */
export interface Delivery {
  /** Deliver a message to an OWNED agent as a new turn (prompt its session). `from`
   *  is the sender, passed structured so the agent can reply to it. */
  deliverAsTurn: (agentId: string, from: string, text: string) => void;
  /** Queue a message for an ATTACHED agent. kild does not own its process, so there is
   *  nothing to prompt: it collects this at its own turn boundary. Structured (`from` +
   *  the raw text), because how it reads is decided where it is drained, not here. */
  queueForAttached: (agent: AttachedAgent, from: string, text: string) => void;
  /** Broadcast a message to all clients so the human (CLI/UI) sees it. */
  broadcast: (message: Message) => void;
}

/** How a delivered message reads to the agent receiving it: who, where, what. */
export function formatDelivery(kildName: string, from: string, text: string): string {
  return `[#${kildName}] @${from}: ${text}`;
}

/** Return resolved addressees that cannot receive a message in this kild. Engine notices
 * are never user-addressed, so they cannot yield a warning. */
export function unknownRecipients(kild: Kild, message: Message): string[] {
  if (message.system) return [];
  const handles = new Set(kild.agents.map((agent) => agent.handle));
  return message.to.filter((recipient) => recipient !== HUMAN && !handles.has(recipient));
}

/**
 * Route one already-recorded message: show it to the human (broadcast), then deliver
 * it as a turn to each addressed agent.
 *
 * **`message.to` is authoritative.** The kild manager resolved it when it recorded the
 * message — that is the one place that answers "who is this addressed to?" (a system
 * notice → nobody, else an explicit `to`, else the kild lead). Addressing is a structured
 * list, never parsed from the message text — the router must never re-derive addressees
 * from prose. Re-deriving would be a second, divergent answer: it once silently overrode a
 * deliberate empty `to`, so a system notice like "@coder joined the kild." prompted
 * @coder with a turn.
 *
 * This function owns only *delivery* policy — who actually gets prompted:
 * - `@human` is not a session, so it's never delivered a turn — the broadcast is how the
 *   operator receives. BUT a `@human` message from a non-lead ALSO wakes the kild lead (the
 *   human's in-kild proxy/coordinator), so a sub-agent "reporting to the human" never
 *   leaves the lead blind. The lead's own messages to `@human` stay terminal (wake no one).
 * - an agent is never delivered its own message;
 * - **no addressee + exactly one agent → that agent** — so a bare message in a
 *   single-agent kild reaches the agent, and it chats like a 1:1 session;
 * - no addressee + multiple agents → broadcast only (no turn).
 *
 * **Notices broadcast but never deliver a turn.** A notice is engine-generated (an agent
 * joining, the kild halting): the operator should see it, but it addresses no one — waking
 * an agent with it is noise at best and, for a halt, the opposite of the intent.
 */
export function routeMessage(kild: Kild, message: Message, delivery: Delivery): void {
  delivery.broadcast(message);
  // Engine notices are shown to the human, never delivered as a turn.
  if (message.system) return;

  const targets = message.to.filter((t) => t !== HUMAN && t !== message.from);
  if (targets.length === 0 && kild.agents.length === 1) {
    const sole = kild.agents[0];
    if (sole && sole.handle !== message.from) targets.push(sole.handle);
  }

  // A `@human` report from a non-lead also wakes the kild lead — so an agent that reports
  // "to the human" keeps the coordinator in the loop instead of leaving it blind. The lead
  // itself reporting to the human stays terminal (it's the sender, so it's not re-woken).
  const lead = kild.agents[0];
  if (
    message.to.includes(HUMAN) &&
    lead &&
    lead.handle !== message.from &&
    !targets.includes(lead.handle)
  ) {
    targets.push(lead.handle);
  }

  for (const handle of targets) {
    const agent = kild.agents.find((a) => a.handle === handle);
    if (!agent) continue;
    // The one branch the attached ownership adds: same recipients, same rules — only the
    // transport differs (push a turn vs queue for the pull).
    if (agent.ownership === 'attached') {
      delivery.queueForAttached(agent, message.from, message.text);
    } else {
      delivery.deliverAsTurn(
        agent.id,
        message.from,
        formatDelivery(kild.name, message.from, message.text),
      );
    }
  }
}
