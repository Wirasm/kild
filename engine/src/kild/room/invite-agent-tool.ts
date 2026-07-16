import type { ToolDefinition } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';

/**
 * Lets an agent pull another agent into its room. A participant registers this
 * (only when running in a room); calling it emits an `invite` control line the
 * engine turns into a new participant — the agent-driven side of "grow a single
 * agent into a room on the fly". The transport is injected (the worker writes the
 * control line to the engine over stdout).
 */
export function createInviteAgentTool(
  emit: (spec: { name: string; agent?: string; model?: string }) => Promise<string>,
): ToolDefinition {
  return {
    name: 'invite_agent',
    label: 'Invite Agent',
    description:
      'Invite another agent into this room as a new participant you can then address ' +
      'with @name. `name` is the @handle; `agent` is the agent definition to run (defaults ' +
      'to the same as the name); `model` is optional.',
    promptSnippet: 'invite_agent — bring another agent into the room, then address it with @name',
    parameters: Type.Object({
      name: Type.String({ description: 'The @handle for the new participant.' }),
      agent: Type.Optional(
        Type.String({ description: 'Agent definition to run (default: name).' }),
      ),
      model: Type.Optional(Type.String({ description: 'Optional model override.' })),
    }),
    async execute(_toolCallId, params) {
      const p = params as { name: string; agent?: string; model?: string };
      const message = await emit({ name: p.name, agent: p.agent ?? p.name, model: p.model });
      return {
        content: [{ type: 'text' as const, text: message }],
        details: null,
      };
    },
  };
}
