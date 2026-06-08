<script lang="ts">
  import type { Participant, Room } from "../types";

  let {
    room,
    participant,
    onClose,
    onHalt,
    onOpenWorktree,
  }: {
    room: Room;
    participant: Participant | null;
    onClose: () => void;
    onHalt: () => void;
    onOpenWorktree: (path: string) => void;
  } = $props();

  let solo = $derived(room.participants.length === 1);
</script>

<header class="topbar" data-tauri-drag-region>
  <span class="project-chip">{room.name}</span>
  {#if solo}
    <span class="summary">
      {room.participants[0]?.name} · {participant?.modelLabel ?? participant?.model ?? "…"}
    </span>
  {:else}
    <span class="summary">{room.participants.length} agents{participant ? ` · @${participant.name}` : ""}</span>
  {/if}
  {#if room.branch}
    {#if room.worktreePath}
      <button
        class="branch-chip"
        title="Open {room.worktreePath}"
        onclick={() => onOpenWorktree(room.worktreePath!)}
      >
        ⎇ {room.branch} ⧉
      </button>
    {:else}
      <span class="branch-chip static">⎇ {room.branch}</span>
    {/if}
  {/if}
  {#if room.status === "stopped"}
    <span class="stopped-tag">stopped</span>
  {/if}
  <span class="spacer" data-tauri-drag-region></span>
  {#if participant?.stats}
    <span class="gauge">
      ctx {participant.stats.context_pct ?? "–"}% · {participant.stats.tokens} tok · ${participant.stats.cost.toFixed(4)}
    </span>
  {/if}
  {#if room.status !== "stopped" && !room.archived}
    <button class="halt-room" title="Halt the agents now (keeps the room read-only)" onclick={onHalt}>
      ⏹ halt
    </button>
  {/if}
  <button class="close-room" title="Close room" onclick={onClose}>✕</button>
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
  .topbar .halt-room {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-muted);
    border-radius: 4px;
    padding: 1px 8px;
    cursor: pointer;
    font-size: 12px;
  }
  .topbar .halt-room:hover {
    color: var(--ember);
    border-color: var(--ember);
    background: rgba(216, 92, 92, 0.06);
  }
  .topbar .close-room {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-muted);
    border-radius: 4px;
    padding: 1px 7px;
    cursor: pointer;
    font-size: 12px;
  }
  .topbar .close-room:hover {
    color: var(--ember);
    border-color: var(--ember);
  }
</style>
