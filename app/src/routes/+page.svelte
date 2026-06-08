<script lang="ts">
  import { onMount } from "svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import RoomView from "$lib/components/RoomView.svelte";
  import ProjectModal from "$lib/components/ProjectModal.svelte";
  import RoomModal from "$lib/components/RoomModal.svelte";
  import {
    EngineSocket,
    addProject as apiAddProject,
    listAgents,
    listProjects,
    listWorktrees,
    listArchivedRooms,
    removeWorktree as apiRemoveWorktree,
    pruneWorktrees as apiPruneWorktrees,
    openWorktree as apiOpenWorktree,
  } from "$lib/api";

  import type { Project, Agent, UiEvent, Room, Message, RoomSummary, RoomSpec, Worktree } from "$lib/types";

  const MODELS = [
    "anthropic/claude-haiku-4-5",
    "anthropic/claude-sonnet-4-6",
    "anthropic/claude-opus-4-8",
    "openai-codex/gpt-5.5",
    "minimax/MiniMax-M3",
  ];

  // Projects + new-room config
  let projects = $state<Project[]>([]);
  let active = $state<Project | null>(null); // active project = context for new rooms
  let adding = $state(false);
  let startingRoom = $state(false);
  let newName = $state("");
  let newPath = $state("");
  let addError = $state<string | null>(null);
  let agents = $state<Agent[]>([]);
  let selectedAgents = $state<string[]>(["default"]); // room participants to spawn
  let model = $state(MODELS[0]);

  // Run-location for a new room: main checkout (default), a new worktree, or an
  // existing one (attach — shared by all participants). `worktrees` = the project's trees.
  let runMode = $state<"main" | "new" | "existing">("main");
  let worktreeName = $state("");
  let existingWorktree = $state("");
  let worktrees = $state<Worktree[]>([]);
  let worktreeNames = $derived(worktrees.map((w) => w.name ?? w.branch));

  $effect(() => {
    if (typeof localStorage !== "undefined") localStorage.setItem("kild_last_model", model);
  });

  // Rooms (runtime registry). A single agent is a 1-participant room.
  let rooms = $state<Room[]>([]);
  let activeRoomId = $state<string | null>(null);
  let activeParticipantId = $state<string | null>(null); // a participant @handle
  let input = $state("");
  let error = $state<string | null>(null);
  let socket: EngineSocket | null = null;

  let activeRoom = $derived(rooms.find((r) => r.id === activeRoomId) ?? null);
  let activeParticipant = $derived(
    activeRoom?.participants.find((p) => p.name === activeParticipantId) ?? null,
  );

  // Route a participant's transcript event (UiEvent) to that participant's items.
  function handle(roomId: string, participantName: string, ev: UiEvent) {
    const room = rooms.find((r) => r.id === roomId);
    const p = room?.participants.find((x) => x.name === participantName);
    if (!p) return;
    if (ev.kind === "text" || ev.kind === "tool_start" || ev.kind === "model") p.running = true;
    switch (ev.kind) {
      case "model":
        p.modelLabel = `${ev.provider} / ${ev.id}`;
        break;
      case "text": {
        const last = p.items[p.items.length - 1];
        if (last && last.type === "assistant") last.text += ev.delta;
        else p.items.push({ type: "assistant", text: ev.delta });
        break;
      }
      case "tool_start":
        p.items.push({ type: "tool", id: ev.id, name: ev.name, args: ev.args, status: "running" });
        break;
      case "tool_end":
        for (let i = p.items.length - 1; i >= 0; i--) {
          const it = p.items[i];
          if (it.type === "tool" && it.id === ev.id && it.status === "running") {
            it.status = ev.ok ? "ok" : "error";
            break;
          }
        }
        break;
      case "stats":
        p.stats = { tokens: ev.tokens, cost: ev.cost, context_pct: ev.context_pct };
        break;
      case "agent_end":
        p.running = false;
        break;
      case "error":
        p.items.push({ type: "assistant", text: `⚠️ ${ev.message}` });
        p.running = false;
        break;
      case "session_end":
        for (const it of p.items) if (it.type === "tool" && it.status === "running") it.status = "error";
        p.running = false;
        break;
      case "retry":
        break;
      default: {
        const _exhaustive: never = ev;
        void _exhaustive;
      }
    }
  }

  // Append a post to the room's shared log (the conversation feed).
  function handleRoomMessage(roomId: string, msg: Message) {
    rooms.find((r) => r.id === roomId)?.log.push(msg);
  }

  async function loadProjects() {
    projects = await listProjects();
  }

  // Merge the engine's on-disk room history into the list as read-only entries. Runs
  // whenever the engine (re)connects — after a dev `--watch` reload the engine loses
  // its live rooms but keeps their logs, so this restores the conversation record.
  async function loadArchivedRooms() {
    try {
      const archived = await listArchivedRooms();
      for (const a of archived) {
        if (rooms.some((r) => r.id === a.id)) continue; // a live/owned room wins
        rooms.push({
          id: a.id,
          name: a.name,
          participants: a.participants.map((p) => ({
            name: p.name,
            agent: p.agent,
            model: "default",
            items: [],
            running: false,
            modelLabel: null,
            stats: null,
          })),
          log: a.log,
          status: "stopped",
          origin: "cli",
          archived: true,
          branch: a.worktree ? `kild/${a.worktree}` : undefined,
        });
      }
    } catch (e) {
      // Past-room history is optional enrichment — never block the cockpit on it.
      console.warn("kild: could not load room history", e);
    }
  }
  async function loadAgents(projectPath: string) {
    agents = await listAgents(projectPath);
    selectedAgents = selectedAgents.filter((a) => agents.some((x) => x.name === a));
    if (selectedAgents.length === 0) selectedAgents = ["default"];
  }
  async function selectProject(p: Project) {
    active = p;
    try {
      await loadAgents(p.path);
    } catch (e) {
      error = `Could not load agents: ${e}`;
    }
    refreshWorktrees();
  }
  async function addProject() {
    addError = null;
    try {
      const p = await apiAddProject(newName.trim(), newPath.trim());
      await loadProjects();
      adding = false;
      newName = "";
      newPath = "";
      await selectProject(p);
    } catch (e) {
      addError = String(e instanceof Error ? e.message : e);
    }
  }

  // Open the new-room modal: load worktrees (so "existing" can attach) + seed a name.
  async function openRoomModal() {
    runMode = "main";
    existingWorktree = "";
    worktreeName = `${selectedAgents[0] ?? "room"}-${crypto.randomUUID().slice(0, 4)}`;
    startingRoom = true;
    if (active) {
      try {
        worktrees = await listWorktrees(active.path);
      } catch (e) {
        worktrees = [];
        error = `Could not load worktrees: ${e instanceof Error ? e.message : e}`;
      }
    }
  }

  function selectedWorktree(): string | undefined {
    if (runMode === "new") return worktreeName.trim() || undefined;
    if (runMode === "existing") return existingWorktree || undefined;
    return undefined;
  }

  const ownedIds = new Set<string>();

  function startRoom() {
    if (!active || !socket || selectedAgents.length === 0) return;
    error = null;
    const worktree = selectedWorktree();
    const spec: RoomSpec = {
      name: active.name,
      cwd: active.path,
      participants: selectedAgents.map((a) => ({ name: a, agent: a, model })),
      worktree,
    };
    const id = crypto.randomUUID();
    socket.openRoom(id, spec);
    ownedIds.add(id);
    rooms.push({
      id,
      name: spec.name,
      participants: spec.participants.map((p) => ({
        name: p.name,
        agent: p.agent,
        model: p.model ?? "default",
        items: [],
        running: false,
        modelLabel: null,
        stats: null,
      })),
      log: [],
      status: "running",
      origin: "ui",
      branch: worktree ? `kild/${worktree}` : undefined,
    });
    activeRoomId = id;
    activeParticipantId = spec.participants[0]?.name ?? null;
    input = "";
    startingRoom = false;
  }

  function selectRoom(id: string) {
    activeRoomId = id;
    activeParticipantId = rooms.find((r) => r.id === id)?.participants[0]?.name ?? null;
    input = "";
  }

  function closeRoom(id: string) {
    socket?.closeRoom(id);
    ownedIds.delete(id);
    rooms = rooms.filter((r) => r.id !== id);
    if (activeRoomId === id) {
      activeRoomId = rooms[rooms.length - 1]?.id ?? null;
      activeParticipantId = null;
    }
  }

  // Manual circuit breaker: stop the room's agents but keep it (read-only). Optimistic —
  // the engine confirms via the next room summary (stopped: true).
  function haltRoom(id: string) {
    socket?.haltRoom(id);
    const room = rooms.find((r) => r.id === id);
    if (room) {
      room.status = "stopped";
      for (const p of room.participants) p.running = false;
    }
  }

  // Post into the active room — addressing the focused participant if no @mention.
  function send() {
    if (!activeRoom || !socket) return;
    const text = input.trim();
    if (!text) return;
    const target = activeParticipant;
    const body = /@[A-Za-z0-9_-]+/.test(text) || !target ? text : `@${target.name} ${text}`;
    socket.postToRoom(activeRoom.id, body);
    input = "";
    if (target) target.running = true; // optimistic spinner
  }

  // Invite an agent into the active room on the fly (turns a single agent into a room).
  function inviteParticipant(agentName: string) {
    if (!activeRoom || !socket) return;
    socket.addParticipant(activeRoom.id, { name: agentName, agent: agentName, model });
  }

  async function refreshWorktrees() {
    if (!active) return;
    try {
      worktrees = await listWorktrees(active.path);
    } catch (e) {
      error = `Could not load worktrees: ${e instanceof Error ? e.message : e}`;
    }
  }
  async function removeWorktree(name: string) {
    if (!active) return;
    try {
      await apiRemoveWorktree(active.path, name);
      await refreshWorktrees();
    } catch (e) {
      error = `Could not remove worktree: ${e instanceof Error ? e.message : e}`;
    }
  }
  async function pruneWorktrees() {
    if (!active) return;
    try {
      await apiPruneWorktrees(active.path);
      await refreshWorktrees();
    } catch (e) {
      error = `Could not prune worktrees: ${e instanceof Error ? e.message : e}`;
    }
  }
  async function onOpenWorktree(path: string) {
    try {
      await apiOpenWorktree(path);
    } catch (e) {
      error = `Could not open worktree: ${e instanceof Error ? e.message : e}`;
    }
  }

  // Mirror the engine's room list: upsert rooms + participants, drop dead ones.
  function reconcileRooms(summaries: RoomSummary[]) {
    const live = new Set(summaries.map((s) => s.id));
    // Keep live + owned (pending the open echo) + archived (read-only history). A
    // formerly-live room the engine no longer has, and which isn't archived, is dropped.
    rooms = rooms.filter((r) => ownedIds.has(r.id) || live.has(r.id) || r.archived);
    for (const sum of summaries) {
      // The engine now knows this room — it's no longer "pending the open echo", so a
      // later restart that wipes the engine's list drops it instead of leaving a ghost.
      ownedIds.delete(sum.id);
      let room = rooms.find((r) => r.id === sum.id);
      if (!room) {
        room = {
          id: sum.id,
          name: sum.name,
          participants: [],
          log: [],
          status: "running",
          origin: "cli",
          branch: sum.worktree ? `kild/${sum.worktree}` : undefined,
        };
        rooms.push(room);
      } else if (sum.worktree && !room.branch) {
        room.branch = `kild/${sum.worktree}`;
      }
      room.status = sum.stopped ? "stopped" : "running"; // engine is the source of truth
      for (const p of sum.participants) {
        if (!room.participants.some((x) => x.name === p.name)) {
          room.participants.push({
            name: p.name,
            agent: p.agent,
            model: "default",
            items: [],
            running: false,
            modelLabel: null,
            stats: null,
          });
        }
      }
    }
    if (activeRoomId && !rooms.some((r) => r.id === activeRoomId)) {
      activeRoomId = rooms[rooms.length - 1]?.id ?? null;
      activeParticipantId = null;
    }
    if (activeRoom && !activeParticipant) {
      activeParticipantId = activeRoom.participants[0]?.name ?? null;
    }
  }

  onMount(() => {
    if (typeof localStorage !== "undefined") {
      const savedModel = localStorage.getItem("kild_last_model");
      if (savedModel && MODELS.includes(savedModel)) model = savedModel;
    }
    socket = new EngineSocket(
      (room, participant, event) => handle(room, participant, event),
      (connected) => {
        if (connected) {
          if (error?.startsWith("Engine")) error = null;
          void loadArchivedRooms(); // restore read-only history once the engine is reachable
        } else {
          for (const r of rooms) {
            for (const p of r.participants) p.running = false;
            r.status = "stopped";
          }
          error = "Engine disconnected — reconnecting…";
        }
      },
      (summaries) => reconcileRooms(summaries),
      (room, msg) => handleRoomMessage(room, msg),
    );
    loadProjects()
      .then(() => {
        if (projects.length > 0) selectProject(projects[0]);
      })
      .catch((e) => (error = `Could not load projects: ${e}`));
    return () => socket?.close();
  });
</script>

<div class="app">
  <Sidebar
    projects={projects}
    bind:active={active}
    bind:adding={adding}
    rooms={rooms}
    activeRoomId={activeRoomId}
    worktrees={worktrees}
    onSelectProject={selectProject}
    onNewRoom={openRoomModal}
    onSelectRoom={selectRoom}
    onCloseRoom={closeRoom}
    onRemoveWorktree={removeWorktree}
    onPruneWorktrees={pruneWorktrees}
  />

  <ProjectModal
    bind:isOpen={adding}
    bind:newName={newName}
    bind:newPath={newPath}
    addError={addError}
    onAdd={addProject}
    onClose={() => (adding = false)}
  />

  <RoomModal
    bind:isOpen={startingRoom}
    agents={agents}
    bind:selectedAgents={selectedAgents}
    bind:model={model}
    models={MODELS}
    projectName={active?.name ?? ""}
    worktrees={worktreeNames}
    bind:runMode={runMode}
    bind:worktreeName={worktreeName}
    bind:existingWorktree={existingWorktree}
    onStart={startRoom}
    onClose={() => (startingRoom = false)}
  />

  <main class="main">
    {#if error}
      <div class="banner">
        <span>{error}</span>
        <button onclick={() => (error = null)}>✕</button>
      </div>
    {/if}

    {#if projects.length === 0}
      <div class="empty">
        <h2>No project yet</h2>
        <p>Add a project directory to start working with agents in it.</p>
        <button class="primary" onclick={() => (adding = true)}>+ add project</button>
      </div>
    {:else if !activeRoom}
      <div class="empty">
        <h2>No room</h2>
        <p>Pick a project, choose an agent (or several), and start a room.</p>
        {#if active}<button class="primary" onclick={openRoomModal}>+ new room</button>{/if}
      </div>
    {:else}
      <RoomView
        room={activeRoom}
        activeParticipant={activeParticipant}
        agents={agents}
        bind:input={input}
        onSelectParticipant={(name) => (activeParticipantId = name)}
        onInvite={inviteParticipant}
        onClose={() => activeRoom && closeRoom(activeRoom.id)}
        onHalt={() => activeRoom && haltRoom(activeRoom.id)}
        onOpenWorktree={onOpenWorktree}
        onSend={send}
      />
    {/if}
  </main>
</div>

<style>
  .app {
    display: grid;
    grid-template-columns: 240px 1fr;
    height: 100vh;
  }
  .main {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    color: var(--text-subtle);
  }
  .empty h2 {
    color: var(--text-bright);
    margin: 0;
  }
  .empty .primary {
    padding: 8px 16px;
    border-radius: 8px;
    cursor: pointer;
    font: inherit;
  }
  .banner {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    padding: 9px 16px;
    background: #2a1719;
    color: var(--text-bright);
    border-bottom: 1px solid var(--ember);
    font-size: 13px;
  }
  .banner button {
    background: transparent;
    border: none;
    color: var(--text-subtle);
    cursor: pointer;
    font: inherit;
  }
</style>
