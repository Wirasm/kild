<script lang="ts">
  import type { Project, Agent, Session } from "../types";

  interface Props {
    projects: Project[];
    active: Project | null;
    adding: boolean;
    newName: string;
    newPath: string;
    addError: string | null;
    agents: Agent[];
    agentName: string;
    model: string;
    models: string[];
    sessions: Session[];
    activeId: number | null;
    onSelectProject: (p: Project) => void;
    onAddProject: () => void;
    onStartSession: () => void;
    onSelectSession: (id: number) => void;
    onCloseSession: (id: number) => void;
  }

  let {
    projects,
    active = $bindable(),
    adding = $bindable(),
    newName = $bindable(),
    newPath = $bindable(),
    addError = $bindable(),
    agents,
    agentName = $bindable(),
    model = $bindable(),
    models,
    sessions = $bindable(),
    activeId = $bindable(),
    onSelectProject,
    onAddProject,
    onStartSession,
    onSelectSession,
    onCloseSession,
  }: Props = $props();
</script>

<aside class="sidebar">
  <div class="brand">kild</div>

  <div class="section-label">Projects</div>
  {#each projects as p}
    <button class="project" class:active={active?.name === p.name} onclick={() => onSelectProject(p)}>
      <span class="p-name">{p.name}</span>
      <span class="p-path">{p.path}</span>
    </button>
  {/each}
  {#if adding}
    <div class="add-form">
      <input bind:value={newName} placeholder="name" />
      <input bind:value={newPath} placeholder="~/projects/my-app" />
      {#if addError}<div class="add-error">{addError}</div>{/if}
      <div class="add-actions">
        <button class="primary" onclick={onAddProject}>Add</button>
        <button onclick={() => (adding = false)}>Cancel</button>
      </div>
    </div>
  {:else}
    <button class="new" onclick={() => (adding = true)}>+ add project</button>
  {/if}

  {#if active}
    <div class="section-label">New session · {active.name}</div>
    <div class="new-session">
      <select bind:value={agentName} title="Agent (system prompt)">
        {#each agents as a}<option value={a.name}>{a.name}</option>{/each}
      </select>
      <select bind:value={model} title="Model">
        {#each models as m}<option value={m}>{m}</option>{/each}
      </select>
      <button class="primary start" onclick={onStartSession}>+ start session</button>
    </div>
  {/if}

  {#if sessions.length > 0}
    <div class="section-label">Sessions</div>
    {#each sessions as s}
      <div class="session-row" class:active={s.id === activeId}>
        <button class="session-pick" onclick={() => onSelectSession(s.id)}>
          <span class="dot {s.status}" class:busy={s.running}></span>
          <span class="s-title">{s.agent} · {s.model}</span>
          <span class="s-proj">{s.projectName}</span>
        </button>
        <button class="session-close" title="Close session" onclick={() => onCloseSession(s.id)}>✕</button>
      </div>
    {/each}
  {/if}
</aside>

<style>
  .sidebar {
    background: var(--obsidian);
    border-right: 1px solid var(--border-subtle);
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    overflow-y: auto;
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
  }
  .project {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .project.active {
    background: var(--surface);
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
  .add-form {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
    background: var(--surface);
    border-radius: 8px;
  }
  .add-form input {
    background: var(--void);
    color: var(--text-bright);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 8px;
    font: inherit;
    font-size: 12px;
  }
  .add-error {
    color: var(--ember);
    font-size: 11px;
  }
  .add-actions {
    display: flex;
    gap: 6px;
  }
  .primary {
    background: var(--ice) !important;
    color: var(--void) !important;
    font-weight: 600;
    border: none !important;
  }
  .new-session {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 2px 4px 4px;
  }
  .new-session select {
    background: var(--surface);
    color: var(--text-bright);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 8px;
    font: inherit;
  }
  .new-session .start {
    text-align: center;
    padding: 7px;
  }
  .session-row {
    display: flex;
    align-items: center;
    border-radius: 6px;
  }
  .session-row.active {
    background: var(--surface);
  }
  .session-pick {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
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
  }
  .dot.running {
    background: var(--aurora);
  }
  .dot.stopped {
    background: var(--text-muted);
  }
  .dot.busy {
    animation: pulse 1s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.3;
    }
  }
  .session-close {
    color: var(--text-muted) !important;
    padding: 4px 8px !important;
    flex: none;
  }
  .session-close:hover {
    color: var(--ember) !important;
  }
</style>
