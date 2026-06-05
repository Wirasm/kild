import path from 'node:path';

/** Default model for the Flue layer's helpers. Interactive sessions resolve their
 *  own model (or pi's configured default) via the coding-agent SDK. */
export const DEFAULT_MODEL = process.env.KILD_MODEL ?? 'minimax/MiniMax-M3';

/** kild's state directory: `$KILD_HOME`, else `$XDG_CONFIG_HOME/kild`, else
 *  `~/.config/kild`. Holds projects.json, worktrees, etc. */
export function kildHome(): string {
  if (process.env.KILD_HOME) return process.env.KILD_HOME;
  const base = process.env.XDG_CONFIG_HOME ?? path.join(process.env.HOME ?? '', '.config');
  return path.join(base, 'kild');
}
