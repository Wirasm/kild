import type { ToolDefinition } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';

/**
 * Lets an agent pull another agent into its kild. An agent registers this (only when
 * running in a kild); calling it emits a `spawn` control line the engine turns into a
 * new agent — the agent-driven side of "grow a single agent into a kild on the fly".
 * The transport is injected (the agent process writes the control line to the engine
 * over stdout).
 */
export function createSpawnTool(
  emit: (spec: {
    handle: string;
    persona?: string;
    model?: string;
    task?: string;
  }) => Promise<string>,
): ToolDefinition {
  return {
    name: 'spawn',
    label: 'Spawn',
    description:
      'Spawn another agent into this kild, which you can then address with @handle. ' +
      '`handle` is the @handle; `persona` is the persona to run (defaults ' +
      'to the same as the handle); `model` is optional. Pass `task` to give it its first ' +
      'assignment — it is delivered as a message from you, so it starts working ' +
      'immediately; without one the agent starts idle and does nothing until you send to it.',
    promptSnippet: 'spawn — bring another agent into the kild, then address it with @handle',
    parameters: Type.Object({
      handle: Type.String({ description: 'The @handle for the new agent.' }),
      persona: Type.Optional(Type.String({ description: 'Persona to run (default: handle).' })),
      model: Type.Optional(Type.String({ description: 'Optional model override.' })),
      task: Type.Optional(
        Type.String({
          description:
            'What this agent is being spawned to do, delivered as its first message from ' +
            'you. Write it as you would a `send`: the goal, the outcome you want, and how ' +
            'it is judged done. It appears on the kild log; revise it later with `send`.',
        }),
      ),
    }),
    async execute(_toolCallId, params) {
      const p = params as { handle: string; persona?: string; model?: string; task?: string };
      const message = await emit({
        handle: p.handle,
        persona: p.persona ?? p.handle,
        model: p.model,
        task: p.task,
      });
      return {
        content: [{ type: 'text' as const, text: message }],
        details: null,
      };
    },
  };
}
