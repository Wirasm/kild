/**
 * The cockpit-facing event stream and its translation from pi agent events.
 *
 * `UiEvent` is identical to what the old Rust `rpc` slice produced; the cockpit
 * routes each to the right transcript by session id. pi wire shapes never reach
 * the UI — translation happens here, at the boundary.
 */
export type UiEvent =
  | { kind: 'model'; provider: string; id: string }
  | { kind: 'text'; delta: string }
  | { kind: 'tool_start'; id: string; name: string; args: string }
  | { kind: 'tool_end'; id: string; name: string; ok: boolean }
  | { kind: 'retry'; attempt: number; max: number }
  | { kind: 'agent_end' }
  | { kind: 'stats'; tokens: number; cost: number; context_pct: number | null }
  | { kind: 'error'; message: string }
  | { kind: 'session_end' };

/** A pi agent event as delivered to `AgentSession.subscribe` (loosely typed at the boundary). */
export interface RawAgentEvent {
  type: string;
  assistantMessageEvent?: { type?: string; delta?: string };
  toolCallId?: string;
  toolName?: string;
  isError?: boolean;
  args?: unknown;
  attempt?: number;
  maxAttempts?: number;
}

export function translate(event: RawAgentEvent): UiEvent | null {
  switch (event.type) {
    case 'message_update':
      if (event.assistantMessageEvent?.type === 'text_delta' && event.assistantMessageEvent.delta) {
        return { kind: 'text', delta: event.assistantMessageEvent.delta };
      }
      return null;
    case 'tool_execution_start':
      return {
        kind: 'tool_start',
        id: event.toolCallId ?? '',
        name: event.toolName ?? '',
        args: JSON.stringify(event.args ?? {}),
      };
    case 'tool_execution_end':
      return {
        kind: 'tool_end',
        id: event.toolCallId ?? '',
        name: event.toolName ?? '',
        ok: !event.isError,
      };
    case 'auto_retry_start':
      return { kind: 'retry', attempt: event.attempt ?? 0, max: event.maxAttempts ?? 0 };
    case 'agent_end':
      return { kind: 'agent_end' };
    default:
      return null;
  }
}
