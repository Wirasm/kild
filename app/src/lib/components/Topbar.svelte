<script lang="ts">
  import type { Session } from "../types";

  let {
    activeSession,
    onOpenWorktree,
  }: { activeSession: Session; onOpenWorktree: (path: string) => void } = $props();
</script>

<header class="topbar" data-tauri-drag-region>
  <span class="project-chip">{activeSession.projectName}</span>
  <span class="summary">{activeSession.agent} · {activeSession.model}</span>
  <span class="model">{activeSession.modelLabel ?? "…"}</span>
  {#if activeSession.branch}
    {#if activeSession.worktreePath}
      <button
        class="branch-chip"
        title="Open {activeSession.worktreePath}"
        onclick={() => onOpenWorktree(activeSession.worktreePath!)}
      >
        ⎇ {activeSession.branch} ⧉
      </button>
    {:else}
      <span class="branch-chip static">⎇ {activeSession.branch}</span>
    {/if}
  {/if}
  {#if activeSession.status === "stopped"}
    <span class="stopped-tag">stopped</span>
  {/if}
  <span class="spacer" data-tauri-drag-region></span>
  {#if activeSession.stats}
    <span class="gauge">
      ctx {activeSession.stats.context_pct ?? "–"}% · {activeSession.stats.tokens} tok · ${activeSession.stats.cost.toFixed(4)}
    </span>
  {/if}
</header>

<style>
  .topbar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 28px 16px 10px 16px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--obsidian);
    user-select: none;
  }
  .project-chip {
    color: var(--ice);
    font-weight: 600;
  }
  .topbar .summary {
    color: var(--text-subtle);
    font-size: 13px;
  }
  .topbar .model {
    color: var(--text-muted);
    font-family: var(--mono);
    font-size: 12px;
  }
  .topbar .branch-chip {
    color: var(--aurora);
    font-family: var(--mono);
    font-size: 11px;
    border: 1px solid rgba(124, 200, 160, 0.35);
    border-radius: 4px;
    padding: 1px 6px;
    background: transparent;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .topbar .branch-chip:not(.static):hover {
    background: rgba(124, 200, 160, 0.1);
    border-color: var(--aurora);
  }
  .topbar .branch-chip.static {
    cursor: default;
  }
  .topbar .stopped-tag {
    color: var(--ember);
    font-size: 11px;
    border: 1px solid var(--ember);
    border-radius: 4px;
    padding: 1px 6px;
  }
  .topbar .spacer {
    flex: 1;
  }
  .topbar .gauge {
    color: var(--text-subtle);
    font-family: var(--mono);
    font-size: 12px;
  }
</style>
