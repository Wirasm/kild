<script lang="ts">
  import type { Project, Session } from "../types";

  interface Props {
    projects: Project[];
    active: Project | null;
    adding: boolean;
    newName: string;
    newPath: string;
    addError: string | null;
    sessions: Session[];
    activeId: string | null;
    onSelectProject: (p: Project) => void;
    onAddProject: () => void;
    onNewSession: () => void;
    onSelectSession: (id: string) => void;
    onCloseSession: (id: string) => void;
  }

  let {
    projects,
    active = $bindable(),
    adding = $bindable(),
    newName = $bindable(),
    newPath = $bindable(),
    addError = $bindable(),
    sessions = $bindable(),
    activeId = $bindable(),
    onSelectProject,
    onAddProject,
    onNewSession,
    onSelectSession,
    onCloseSession,
  }: Props = $props();

  let filterMode = $state<"project" | "all">("project");

  // Reset filter mode to show current active project sessions by default
  $effect(() => {
    if (active) {
      filterMode = "project";
    }
  });

  let filteredSessions = $derived(
    active && filterMode === "project"
      ? sessions.filter((s) => s.projectName === active.name)
      : sessions
  );
</script>

<aside class="sidebar">
  <div class="brand">kild</div>

  <div class="section-label">Projects</div>
  {#each projects as p (p.name)}
    <button class="project" class:active={active?.name === p.name} onclick={() => onSelectProject(p)}>
      <span class="p-name">{p.name}</span>
      <span class="p-path">{p.path}</span>
    </button>
  {/each}
  <button class="new" onclick={() => (adding = true)}>+ add project</button>

  <div class="sessions-header-row">
    <div class="section-label">Sessions</div>
    {#if active}
      <div class="filter-toggle">
        <button class:selected={filterMode === "project"} onclick={() => filterMode = "project"}>
          {active.name}
        </button>
        <button class:selected={filterMode === "all"} onclick={() => filterMode = "all"}>
          all
        </button>
      </div>
    {/if}
  </div>
  {#if active && filterMode === "project"}
    <button class="new" onclick={onNewSession}>
      + new session
    </button>
  {/if}

  {#each filteredSessions as s (s.id)}
    <div class="session-row" class:active={s.id === activeId}>
      <button class="session-pick" onclick={() => onSelectSession(s.id)}>
        <span class="dot {s.status}" class:busy={s.running}></span>
        <span class="s-title">{s.agent} · {s.model}</span>
        <span class="s-proj">{s.projectName}</span>
      </button>
      <button class="session-close" title="Close session" onclick={() => onCloseSession(s.id)}>✕</button>
    </div>
  {/each}
</aside>

<style>
  .sidebar {
    background: var(--obsidian-translucent);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    border-right: 1px solid var(--border);
    box-shadow: 4px 0 24px rgba(0, 0, 0, 0.45);
    padding: 28px 12px 12px 12px; /* Offset for macOS traffic lights */
    display: flex;
    flex-direction: column;
    gap: 4px;
    overflow-y: auto;
    user-select: none;
    z-index: 10; /* Cast shadow on top of main panel */
  }
  .brand {
    font-weight: 600;
    color: var(--ice);
    letter-spacing: 0.5px;
    padding: 6px 8px 10px;
  }
  .section-label {
    color: var(--text-muted);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    padding: 12px 8px 4px;
  }
  .sidebar button {
    text-align: left;
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-subtle);
    padding: 7px 9px;
    border-radius: 6px;
    cursor: pointer;
    font: inherit;
    transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
  }
  .sidebar button:hover {
    color: var(--text-bright);
    background: rgba(255, 255, 255, 0.03);
    border-color: var(--border-subtle);
  }
  .project {
    display: flex;
    flex-direction: column;
    gap: 2px;
    border: 1px solid transparent !important;
  }
  .project.active {
    background: var(--surface-translucent);
    border-color: rgba(124, 180, 200, 0.2) !important;
    box-shadow: var(--shadow-subtle);
  }
  .project.active .p-name {
    color: var(--ice);
  }
  .project .p-name {
    color: var(--text-bright);
  }
  .project .p-path {
    color: var(--text-muted);
    font-family: var(--mono);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .new {
    color: var(--text-muted);
    border: 1px dashed var(--border) !important;
  }
  .new:hover {
    border-style: solid !important;
    color: var(--ice) !important;
    border-color: var(--ice) !important;
  }
  .primary {
    background: var(--ice) !important;
    color: var(--void) !important;
    font-weight: 600;
    border: none !important;
  }
  .primary:hover {
    background: var(--ice-dim) !important;
    box-shadow: var(--glow-ice);
  }
  .session-row {
    display: flex;
    align-items: center;
    border-radius: 6px;
    border: 1px solid transparent;
    transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
  }
  .session-row.active {
    background: var(--surface-translucent);
    border: 1px solid rgba(124, 180, 200, 0.25);
    box-shadow: var(--glow-ice);
  }
  .session-pick {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    border: none !important;
  }
  .session-pick:hover {
    background: transparent !important;
    border-color: transparent !important;
  }
  .session-pick .s-title {
    color: var(--text-bright);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .session-pick .s-proj {
    color: var(--text-muted);
    font-size: 11px;
    margin-left: auto;
    flex: none;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex: none;
    position: relative;
    transition: all 0.2s ease;
  }
  .dot.running {
    background: var(--aurora);
    box-shadow: 0 0 6px var(--aurora);
  }
  .dot.stopped {
    background: var(--text-muted);
  }
  .dot.busy {
    background: var(--aurora);
    box-shadow: 0 0 6px var(--aurora);
  }
  .dot.busy::after {
    content: "";
    position: absolute;
    top: -2px;
    left: -2px;
    right: -2px;
    bottom: -2px;
    border: 1px solid var(--aurora);
    border-radius: 50%;
    animation: ripple 1.6s cubic-bezier(0.25, 0, 0, 1) infinite;
  }
  @keyframes ripple {
    0% {
      transform: scale(1);
      opacity: 0.8;
    }
    100% {
      transform: scale(2.2);
      opacity: 0;
    }
  }
  .session-close {
    color: var(--text-muted) !important;
    padding: 4px 8px !important;
    flex: none;
    border: none !important;
  }
  .session-close:hover {
    color: var(--ember) !important;
    background: transparent !important;
  }
  .sessions-header-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-top: 8px;
  }
  .sessions-header-row .section-label {
    padding: 4px 8px;
    margin: 0;
  }
  .filter-toggle {
    display: flex;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid var(--border-subtle);
    border-radius: 12px;
    padding: 2px;
    margin-right: 8px;
  }
  .filter-toggle button {
    background: transparent !important;
    border: none !important;
    color: var(--text-muted) !important;
    font-size: 10px !important;
    font-weight: 500 !important;
    padding: 2px 8px !important;
    border-radius: 10px !important;
    cursor: pointer !important;
    transition: all 0.15s ease !important;
    max-width: 80px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: center;
  }
  .filter-toggle button.selected {
    background: var(--surface) !important;
    color: var(--ice) !important;
    box-shadow: var(--shadow-subtle);
  }
  .filter-toggle button:hover:not(.selected) {
    color: var(--text-bright) !important;
  }
</style>
