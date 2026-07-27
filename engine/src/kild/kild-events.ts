import { agentProcessId, type Kild, type Message } from './kild-types.ts';

/** Stable fallback when a lifecycle event has no agent/human-authored message to report. */
export const NO_FINAL_POST = '(no non-system posts recorded)';

export type KildOperatorEvent =
  | { kind: 'human_post'; from: string; text: string }
  | { kind: 'halted'; finalPost: string }
  | { kind: 'closed'; finalPost: string };

/** The final meaningful kild message — engine notices are lifecycle metadata, not work
 *  output. */
export function finalNonSystemPost(kild: Pick<Kild, 'log'>): string {
  for (let index = kild.log.length - 1; index >= 0; index -= 1) {
    const message = kild.log[index];
    if (message && !message.system) return message.text;
  }
  return NO_FINAL_POST;
}

/** Return the creator only when it is outside the kild, avoiding self-directed turns. */
export function openerNotificationTarget(
  kild: Pick<Kild, 'openedBy' | 'agents'>,
): string | undefined {
  if (!kild.openedBy) return undefined;
  return kild.agents.some((agent) => agentProcessId(agent) === kild.openedBy)
    ? undefined
    : kild.openedBy;
}

/** A direct AgentManager prompt, deliberately distinct from a Message/kild delivery. */
export function formatOperatorNotification(kildName: string, event: KildOperatorEvent): string {
  const label = `[kild operator notification] Kild '${kildName}'`;
  if (event.kind === 'human_post') {
    return `${label}: @${event.from} sent to @human: ${event.text}`;
  }
  const state = event.kind === 'halted' ? 'was halted' : 'was stopped and archived';
  return `${label} ${state}. Final non-system post: ${event.finalPost}`;
}

export function humanPostEvent(message: Message): KildOperatorEvent {
  return { kind: 'human_post', from: message.from, text: message.text };
}
