import { createAgent, type FlueContext } from '@flue/runtime';

import { bridgePiAuth } from '../kild/auth.ts';

/**
 * THE GO/NO-GO GATE — does the user's pi OAuth (Claude Max / ChatGPT) drive Flue?
 *
 * flue run auth-test --target node --payload '{"model":"anthropic/claude-haiku-4-5"}'
 */
export async function run({ init, payload }: FlueContext) {
  const bridged = await bridgePiAuth();
  const model = ((payload ?? {}) as { model?: string }).model ?? 'anthropic/claude-haiku-4-5';

  const agent = createAgent(() => ({ model }));
  const session = await (await init(agent)).session();
  const res = await session.prompt('Reply with exactly five words confirming you are running.');

  return { bridged, modelUsed: `${res.model.provider}/${res.model.id}`, text: res.text };
}
