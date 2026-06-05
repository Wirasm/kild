import {
  type AgentSession,
  AuthStorage,
  createAgentSession,
  ModelRegistry,
} from '@earendil-works/pi-coding-agent';

import { resolveAgentInstructions } from './agents.ts';

/**
 * Frontend-facing event — a translated, serializable view of a pi agent event,
 * identical in shape to what the old Rust `rpc` slice produced. The cockpit
 * routes each to the right transcript by session id. pi wire shapes never reach
 * the UI; translation happens here, at the boundary.
 */
export type UiEvent =
  | { kind: 'model'; provider: string; id: string }
  | { kind: 'text'; delta: string }
  | { kind: 'tool_start'; id: string; name: string; args: string }
  | { kind: 'tool_end'; id: string; name: string; ok: boolean }
  | { kind: 'retry'; attempt: number; max: number }
  | { kind: 'agent_end' }
  | { kind: 'stats'; tokens: number; cost: number; context_pct: number | null }
  | { kind: 'session_end' };

export interface SpawnRequest {
  model?: string;
  cwd?: string;
  agent?: string;
}

/** A pi agent event as delivered to `AgentSession.subscribe` (loosely typed at the boundary). */
interface RawAgentEvent {
  type: string;
  assistantMessageEvent?: { type?: string; delta?: string };
  toolCallId?: string;
  toolName?: string;
  isError?: boolean;
  args?: unknown;
  attempt?: number;
  maxAttempts?: number;
}

/**
 * Owns the live pi agent sessions for the cockpit — the in-process replacement
 * for the old daemon's subprocess registry. Each session is a coding-agent SDK
 * `AgentSession` (native pi auth, no bridge); its event stream is translated to
 * [`UiEvent`] and pushed to the caller's `emit`.
 */
export class SessionManager {
  private readonly authStorage = AuthStorage.create();
  private readonly registry = ModelRegistry.create(this.authStorage);
  private readonly sessions = new Map<string, AgentSession>();

  /** Resolve a `provider/id` or bare `id` pattern to a Model (undefined → pi default). */
  private resolveModel(pattern?: string) {
    if (!pattern) return undefined;
    const slash = pattern.indexOf('/');
    if (slash !== -1) {
      return this.registry.find(pattern.slice(0, slash), pattern.slice(slash + 1));
    }
    return this.registry.getAll().find((m) => m.id === pattern);
  }

  async spawn(id: string, req: SpawnRequest, emit: (event: UiEvent) => void): Promise<void> {
    const model = this.resolveModel(req.model);
    const { session } = await createAgentSession({
      model,
      authStorage: this.authStorage,
      modelRegistry: this.registry,
      cwd: req.cwd ?? process.cwd(),
    });
    this.sessions.set(id, session);

    if (model) emit({ kind: 'model', provider: model.provider, id: model.id });

    session.subscribe((event: RawAgentEvent) => {
      const ui = translate(event);
      if (ui) emit(ui);
      if (event.type === 'agent_end') {
        const stats = session.getSessionStats();
        emit({
          kind: 'stats',
          tokens: stats.tokens.total,
          cost: stats.cost,
          context_pct: stats.contextUsage?.percent ?? null,
        });
      }
    });

    // A non-default agent layers its role prompt. The SDK has no
    // append-system-prompt knob, so we prepend the role to the agent's context
    // as a leading instruction. (default agent = pi's own prompt.)
    if (req.agent) {
      const instructions = await resolveAgentInstructions(req.agent, req.cwd);
      if (instructions) this.preambles.set(id, instructions);
    }
  }

  private readonly preambles = new Map<string, string>();

  async prompt(id: string, text: string): Promise<void> {
    const session = this.sessions.get(id);
    if (!session) throw new Error(`no such session: ${id}`);
    const preamble = this.preambles.get(id);
    if (preamble) {
      this.preambles.delete(id);
      await session.prompt(`<role>\n${preamble}\n</role>\n\n${text}`);
    } else {
      await session.prompt(text);
    }
  }

  stop(id: string, emit: (event: UiEvent) => void): void {
    const session = this.sessions.get(id);
    if (session) {
      session.dispose();
      this.sessions.delete(id);
      this.preambles.delete(id);
      emit({ kind: 'session_end' });
    }
  }
}

function translate(event: RawAgentEvent): UiEvent | null {
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
