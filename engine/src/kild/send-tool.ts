import type { ToolDefinition } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';

/**
 * The agent-facing side of a kild: a `send` tool an agent registers (only when it runs
 * in a kild). Calling it is the ONLY way an agent's words reach another agent — we never
 * scrape the agent's prose, which is the failure mode of pi's coms-net POC.
 *
 * **`to` is required.** Addressing is a structured list of handles, never parsed from the
 * message text (so a body can contain `@decorator` / `@media` / an email without
 * misrouting) and never inferred by the engine (so "who did that message go to?" has one
 * answer: whoever the sender named). A send with no recipient is rejected, not redirected.
 *
 * **It is also the only way an agent grows its kild.** Addressing a handle nobody holds
 * creates that agent and delivers to it; `persona` and `model` say who it should be. There
 * used to be a separate `spawn` tool, and the split earned nothing: spawning was never the
 * goal, every real delegation was spawn-then-send, and a spawn on its own left an agent
 * sitting idle — a state that is now unreachable rather than warned about.
 *
 * The transport is injected: the agent process passes an `emit` that writes a `send`
 * control line to the engine.
 */
export function createSendTool(
  emit: (spec: { to: string[]; text: string; persona?: string; model?: string }) => Promise<string>,
): ToolDefinition {
  return {
    name: 'send',
    label: 'Send',
    description:
      'Send a message to agents in your kild, and bring in the ones that are not there ' +
      'yet. Set `to` to the handles you are addressing (e.g. `["coder"]`, or several at ' +
      'once) — exactly those agents are prompted with your message. `to` is REQUIRED: ' +
      'there is no default recipient, and a send that names no one is rejected. This is ' +
      'the ONLY way others see your message — your normal output is private to you.\n\n' +
      'A handle nobody holds yet is CREATED and then sent to, which is how you delegate: ' +
      'pick a handle, say who it should be with `persona`, and write the assignment as the ' +
      'message. It starts working immediately. Pick handles you can tell apart — the same ' +
      'persona under two handles is two agents you can address individually.',
    promptSnippet:
      'send — message agents in your kild; a handle nobody holds yet is created and sent to',
    parameters: Type.Object({
      to: Type.Array(Type.String(), {
        description:
          'Handles to address, e.g. ["coder"] or ["reviewer-auth", "reviewer-api"]. Exactly ' +
          'those agents are prompted with this message. Any handle not already in the kild ' +
          'is created first. Must name at least one.',
      }),
      text: Type.String({
        description:
          'The message body. For an agent you are creating this is its assignment: the goal, ' +
          'the outcome you want, and how it is judged done.',
      }),
      persona: Type.Optional(
        Type.String({
          description:
            'Persona for any recipient that has to be created (ignored for agents already ' +
            'in the kild). Defaults to the handle itself, so `to: ["reviewer"]` runs the ' +
            '`reviewer` persona. Naming a persona that does not exist is rejected — nothing ' +
            'is created and nothing is sent.',
        }),
      ),
      model: Type.Optional(
        Type.String({ description: 'Model for any recipient that has to be created.' }),
      ),
    }),
    async execute(_toolCallId, params) {
      const p = params as { to: string[]; text: string; persona?: string; model?: string };
      const message = await emit({
        to: p.to,
        text: p.text,
        persona: p.persona,
        model: p.model,
      });
      return {
        content: [{ type: 'text' as const, text: message }],
        details: null,
      };
    },
  };
}
