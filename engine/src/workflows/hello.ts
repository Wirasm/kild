import { createAgent, type FlueContext } from '@flue/runtime';

// Smoke test: prove the toolchain + LLM auth work end-to-end before building
// anything else. `flue run hello --target node --payload '{"text":"..."}'`.
export async function run({ init, payload }: FlueContext) {
  const p = (payload ?? {}) as { text?: string; model?: string };
  const agent = createAgent(() => ({ model: p.model ?? 'anthropic/claude-haiku-4-5' }));
  const harness = await init(agent);
  const session = await harness.session();
  const res = await session.prompt(p.text ?? 'Reply with exactly: hello from flue');
  return { text: res.text };
}
