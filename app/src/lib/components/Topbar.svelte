<script lang="ts">
  type Item =
    | { type: "user"; text: string }
    | { type: "assistant"; text: string }
    | { type: "tool"; id: string; name: string; args: string; status: "running" | "ok" | "error" };

  type Session = {
    id: number;
    projectName: string;
    agent: string;
    model: string;
    items: Item[];
    running: boolean;
    status: "running" | "stopped";
    modelLabel: string | null;
    stats: { tokens: number; cost: number; context_pct: number | null } | null;
  };

  let { activeSession }: { activeSession: Session } = $props();
</script>

<header class="topbar">
  <span class="project-chip">{activeSession.projectName}</span>
  <span class="summary">{activeSession.agent} · {activeSession.model}</span>
  <span class="model">{activeSession.modelLabel ?? "…"}</span>
  {#if activeSession.status === "stopped"}
    <span class="stopped-tag">stopped</span>
  {/if}
  <span class="spacer"></span>
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
    padding: 10px 16px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--obsidian);
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
