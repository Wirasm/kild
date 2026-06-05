<script lang="ts">
  import { onMount } from "svelte";

  interface Option {
    value: string;
    label: string;
  }

  interface Props {
    value: string;
    options: (string | Option)[];
    label?: string;
  }

  let { value = $bindable(), options, label = "Select option" }: Props = $props();

  // Normalize options to { value, label } structure
  let normalizedOptions = $derived(
    options.map((opt) => {
      if (typeof opt === "string") {
        return { value: opt, label: opt };
      }
      return opt;
    })
  );

  let selectedLabel = $derived(
    normalizedOptions.find((o) => o.value === value)?.label ?? value
  );

  let isOpen = $state(false);
  let dropdownEl: HTMLDivElement | undefined = $state();

  function toggle() {
    isOpen = !isOpen;
  }

  function select(optValue: string) {
    value = optValue;
    isOpen = false;
  }

  // Handle clicking outside to close the dropdown
  function handleClickOutside(event: MouseEvent) {
    if (isOpen && dropdownEl && !dropdownEl.contains(event.target as Node)) {
      isOpen = false;
    }
  }

  onMount(() => {
    document.addEventListener("click", handleClickOutside);
    return () => {
      document.removeEventListener("click", handleClickOutside);
    };
  });
</script>

<div class="dropdown" bind:this={dropdownEl} class:open={isOpen}>
  <button class="dropdown-trigger" type="button" onclick={toggle} aria-label={label}>
    <span class="value-text">{selectedLabel}</span>
    <span class="chevron" class:open={isOpen}>▾</span>
  </button>

  {#if isOpen}
    <div class="dropdown-menu">
      {#each normalizedOptions as opt}
        <button
          class="dropdown-item"
          class:selected={opt.value === value}
          type="button"
          onclick={() => select(opt.value)}
        >
          {opt.label}
          {#if opt.value === value}
            <span class="check-mark">✓</span>
          {/if}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .dropdown {
    position: relative;
    width: 100%;
    font-family: var(--ui);
  }
  .dropdown-trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    background: var(--surface);
    color: var(--text-bright);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 7px 10px;
    font-size: 13px;
    cursor: pointer;
    text-align: left;
    transition: all 0.2s ease;
  }
  .dropdown-trigger:hover {
    border-color: var(--border-focus);
    background: var(--elevated);
  }
  .dropdown.open .dropdown-trigger {
    border-color: var(--border-focus);
    box-shadow: var(--glow-ice);
  }

  .chevron {
    font-size: 11px;
    color: var(--text-muted);
    transition: transform 0.2s ease;
  }
  .chevron.open {
    transform: rotate(-180deg);
  }

  .dropdown-menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    width: 100%;
    background: var(--obsidian-translucent);
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: var(--shadow-modal);
    z-index: 1000;
    max-height: 200px;
    overflow-y: auto;
    padding: 4px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .dropdown-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    background: transparent;
    border: none;
    color: var(--text);
    padding: 6px 10px;
    font-size: 12px;
    border-radius: 4px;
    cursor: pointer;
    text-align: left;
    transition: all 0.15s ease;
  }
  .dropdown-item:hover {
    background: rgba(255, 255, 255, 0.05);
    color: var(--text-bright);
  }
  .dropdown-item.selected {
    background: rgba(124, 180, 200, 0.1);
    color: var(--ice);
    font-weight: 500;
  }

  .check-mark {
    color: var(--ice);
    font-weight: bold;
    font-size: 10px;
  }
</style>
