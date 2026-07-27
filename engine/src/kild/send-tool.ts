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
 * The transport is injected: the agent process passes an `emit` that writes a `send`
 * control line to the engine.
 */
export function createSendTool(
  emit: (to: string[], text: string) => Promise<string>,
): ToolDefinition {
  return {
    name: 'send',
    label: 'Send',
    description:
      'Send a message to named agents in your kild. Set `to` to the handles you are ' +
      'addressing (e.g. `["coder"]`, or several at once) — exactly those agents are ' +
      'prompted with your message. `to` is REQUIRED: there is no default recipient, and ' +
      'a send that names no one is rejected. This is the ONLY way others see your ' +
      'message — your normal output is private to you.',
    promptSnippet: 'send — message named agents in your kild; `to` is required',
    parameters: Type.Object({
      to: Type.Array(Type.String(), {
        description:
          'Handles to address, e.g. ["coder"] or ["coder", "reviewer"]. Exactly those ' +
          'agents are prompted with this message. Must name at least one agent.',
      }),
      text: Type.String({ description: 'The message body.' }),
    }),
    async execute(_toolCallId, params) {
      const { text, to } = params as { text: string; to: string[] };
      const message = await emit(to, text);
      return {
        content: [{ type: 'text' as const, text: message }],
        details: null,
      };
    },
  };
}
