import type { FlueContext } from '@flue/runtime';

import { findProject } from '../kild/projects.ts';
import { runToCompletion } from '../kild/run.ts';

/** `kild run` rebuilt on Flue: one-shot agent task → RunOutcome.
 *  flue run run --payload '{"prompt":"...","project":"flue-spike","agent":"reviewer"}' */
export async function run({ init, payload }: FlueContext) {
  const p = (payload ?? {}) as { prompt?: string; project?: string; agent?: string; model?: string };
  const projectPath = p.project ? (await findProject(p.project))?.path : undefined;

  return await runToCompletion(init, {
    prompt: p.prompt ?? 'Say hello and name one file in the current directory using your tools.',
    projectPath,
    agent: p.agent,
    model: p.model,
  });
}
