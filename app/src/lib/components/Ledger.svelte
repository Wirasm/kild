<script lang="ts">
  import ToolCard from "./ToolCard.svelte";

  import type { Item } from "../types";

  interface Props {
    items: Item[];
    running: boolean;
  }

  let { items, running }: Props = $props();
  let transcriptEl: HTMLElement | undefined = $state();

  function scrollDown() {
    queueMicrotask(() => transcriptEl?.scrollTo({ top: transcriptEl.scrollHeight }));
  }

  // Svelte 5 effect to automatically scroll to bottom on updates
  $effect(() => {
    if (items.length || running) {
      scrollDown();
    }
  });
</script>

<section class="transcript" bind:this={transcriptEl}>
  {#each items as item}
    {#if item.type === "user"}
      <div class="msg user">{item.text}</div>
    {:else if item.type === "assistant"}
      <div class="msg assistant">{item.text}</div>
    {:else}
      <ToolCard name={item.name} args={item.args} status={item.status} />
    {/if}
  {/each}
  {#if running}
    <div class="thinking-container">
      <div class="spinner"></div>
      <span class="thinking-text">thinking…</span>
    </div>
  {/if}
</section>

<style>
  .transcript {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .msg {
    max-width: 70ch;
    padding: 10px 14px;
    border-radius: 10px;
    white-space: pre-wrap;
    line-height: 1.55;
  }
  .msg.user {
    align-self: flex-end;
    background: var(--ice-dim);
    color: var(--void);
  }
  .msg.assistant {
    align-self: flex-start;
    background: var(--surface);
    color: var(--text-bright);
  }
  .thinking-container {
    align-self: flex-start;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: var(--surface);
    border: 1px solid var(--border-subtle);
    border-radius: 20px;
    color: var(--text-subtle);
    font-size: 11px;
    font-family: var(--ui);
    box-shadow: var(--shadow-subtle);
  }
  .spinner {
    width: 12px;
    height: 12px;
    border: 2px solid rgba(124, 180, 200, 0.2);
    border-top: 2px solid var(--ice);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    0% {
      transform: rotate(0deg);
    }
    100% {
      transform: rotate(360deg);
    }
  }
</style>
