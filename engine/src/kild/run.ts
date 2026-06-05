import type { FlueContext } from '@flue/runtime';
import { createAgent, defineAgentProfile } from '@flue/runtime';
import { local } from '@flue/runtime/node';

import { resolveAgentInstructions } from './agents.ts';
import { DEFAULT_MODEL } from './config.ts';

/** Aggregated result of a one-shot run — mirror of kild-core::rpc::RunOutcome. */
export interface RunOutcome {
  model: string;
  text: string;
  tokens: number;
  cost: number;
}

export interface RunRequest {
  prompt: string;
  projectPath?: string;
  agent?: string;
  model?: string;
}

type Init = FlueContext['init'];

/** Spawn an agent, run one prompt to completion, aggregate. The Flue counterpart
 *  of kild's `run_to_completion` — but ~20 lines instead of a subprocess + JSONL
 *  reader + event translator, because pi-agent-core is in-process here. */
export async function runToCompletion(init: Init, req: RunRequest): Promise<RunOutcome> {
  const instructions = req.agent
    ? await resolveAgentInstructions(req.agent, req.projectPath)
    : null;

  const agent = createAgent(() => ({
    model: req.model ?? DEFAULT_MODEL,
    sandbox: req.projectPath ? local({ cwd: req.projectPath }) : undefined,
    profile: instructions ? defineAgentProfile({ instructions }) : undefined,
  }));

  const harness = await init(agent);
  const session = await harness.session();
  const res = await session.prompt(req.prompt);

  return {
    model: `${res.model.provider}/${res.model.id}`,
    text: res.text,
    tokens: res.usage.totalTokens,
    cost: res.usage.cost.total,
  };
}
