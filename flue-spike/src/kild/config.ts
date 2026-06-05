import path from 'node:path';

/** Default model for the spike. minimax has an API key in pi's auth.json and an
 *  Anthropic-compatible endpoint, so it works without provider OAuth. */
export const DEFAULT_MODEL = process.env.KILD_MODEL ?? 'minimax/MiniMax-M3';

/** Local state dir for the spike: projects.json, worktrees, etc. */
export function kildHome(): string {
  return process.env.KILD_SPIKE_HOME ?? path.resolve(process.cwd(), '.kild-spike');
}
