import { createAgent, type FlueContext } from '@flue/runtime';
import * as v from 'valibot';

import { DEFAULT_MODEL } from '../kild/config.ts';

interface PR {
  number: number;
  title: string;
  files: string[];
}

const MOCK_PRS: PR[] = [
  { number: 1, title: 'Add OAuth login', files: ['src/auth.ts', 'src/routes.ts'] },
  { number: 2, title: 'Refactor routes', files: ['src/routes.ts'] },
  { number: 3, title: 'Update README', files: ['README.md'] },
];

/**
 * VISION SLICE — the merge agent team.
 *
 * A reviewer agent per PR runs in parallel (separate harnesses), each returning
 * a typed risk verdict. Then a deterministic pass proposes a conflict-aware
 * merge order (PRs sharing a file must rebase rather than merge in parallel) —
 * the data the kild UI would render as a merge queue. Mocked PRs keep it
 * deterministic and free of `gh` auth; swap in `gh pr list --json` for real.
 */
export async function run({ init, payload }: FlueContext) {
  const prs = ((payload ?? {}) as { prs?: PR[] }).prs ?? MOCK_PRS;

  const agent = createAgent(() => ({ model: DEFAULT_MODEL }));

  const reviews = await Promise.all(
    prs.map(async (pr) => {
      const session = await (await init(agent, { name: `review-${pr.number}` })).session();
      const res = await session.prompt(
        `You are a merge reviewer. PR #${pr.number} "${pr.title}" touches: ${pr.files.join(', ')}. ` +
          'Give a one-line risk assessment.',
        {
          result: v.object({
            pr: v.number(),
            risk: v.picklist(['low', 'medium', 'high']),
            note: v.string(),
          }),
        },
      );
      return res.data;
    }),
  );

  return { reviews, mergeOrder: proposeMergeOrder(prs) };
}

/** Conflict-aware ordering: fewest-files first; flag file overlaps to rebase. */
function proposeMergeOrder(prs: PR[]): { number: number; action: string }[] {
  const sorted = [...prs].sort((a, b) => a.files.length - b.files.length);
  const seen = new Set<string>();
  return sorted.map((pr) => {
    const conflict = pr.files.some((f) => seen.has(f));
    for (const f of pr.files) seen.add(f);
    return {
      number: pr.number,
      action: conflict ? 'rebase (shares files with earlier PR)' : 'merge clean',
    };
  });
}
