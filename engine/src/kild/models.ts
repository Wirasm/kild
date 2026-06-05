import type { ModelRegistry } from '@earendil-works/pi-coding-agent';

/**
 * Resolve a `provider/id` or bare `id` pattern to a Model. Throws on a
 * given-but-unknown pattern (no silent fallback to pi's default); returns
 * undefined only when no pattern is given (use pi's configured default).
 */
export function resolveModel(registry: ModelRegistry, pattern: string | undefined) {
  if (!pattern) return undefined;
  const slash = pattern.indexOf('/');
  const model =
    slash !== -1
      ? registry.find(pattern.slice(0, slash), pattern.slice(slash + 1))
      : registry.getAll().find((m) => m.id === pattern);
  if (!model) throw new Error(`unknown model: ${pattern}`);
  return model;
}

/** Layer an agent's role prompt onto a user prompt. The coding-agent SDK has no
 *  append-system-prompt knob, so the role is prepended to the first message. */
export function withRole(text: string, instructions: string | null): string {
  return instructions ? `<role>\n${instructions}\n</role>\n\n${text}` : text;
}
