import { observe } from '@flue/runtime';
import type { FlueEvent } from '@flue/runtime';

/** A normalized cockpit event — what the kild UI would render. */
export interface CockpitEvent {
  t: string;
  detail?: string;
}

/**
 * Subscribe to Flue's runtime event stream and normalize it into a cockpit log.
 * This is the observability battery: Flue emits a rich, typed event stream
 * (run/turn/message/tool/agent lifecycle) out of the box — the same shape kild's
 * `rpc` slice hand-translates from pi's RPC events. The cockpit is a subscriber.
 */
export function startCockpitLog(): CockpitEvent[] {
  const log: CockpitEvent[] = [];
  observe((event: FlueEvent) => {
    log.push(normalize(event));
  });
  return log;
}

function normalize(event: FlueEvent): CockpitEvent {
  switch (event.type) {
    case 'run_start':
      return { t: 'run_start', detail: event.workflowName };
    case 'turn_request':
      return { t: 'model', detail: `${event.provider}/${event.model}` };
    case 'tool_execution_start':
      return { t: 'tool_start', detail: event.toolName };
    case 'agent_end':
      return { t: 'agent_end' };
    default:
      return { t: event.type };
  }
}
