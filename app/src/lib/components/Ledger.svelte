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
    <div class="thinking">▍</div>
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
  .thinking {
    color: var(--ice);
    animation: blink 1s steps(2) infinite;
  }
  @keyframes blink {
    50% {
      opacity: 0.25;
    }
  }
</style>
