<script lang="ts">
  import type { Snippet } from 'svelte'
  import { X } from '@lucide/svelte'

  interface Props {
    children: Snippet
    icon?: Snippet
    onremove?: () => void
    variant?: 'neutral' | 'accent' | 'muted'
  }

  let { children, icon, onremove, variant = 'neutral' }: Props = $props()
</script>

<span class="chip {variant}">
  {#if icon}
    <span class="chip-icon">{@render icon()}</span>
  {/if}
  <span class="chip-label">{@render children()}</span>
  {#if onremove}
    <button class="chip-remove" type="button" aria-label="Remove" onclick={onremove}>
      <X size={10} strokeWidth={2.5} />
    </button>
  {/if}
</span>

<style>
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 22px;
    padding: 0 4px 0 8px;
    border-radius: 11px;
    background: var(--c-bg-elevated);
    border: 1px solid var(--c-border-subtle);
    color: var(--c-text-secondary);
    font-size: 11.5px;
    line-height: 1;
    max-width: 240px;
  }

  .chip.accent {
    background: var(--c-accent-bg);
    border-color: var(--c-accent-muted);
    color: var(--c-accent-text);
  }

  .chip.muted {
    background: transparent;
    border-color: var(--c-border);
    color: var(--c-text-muted);
  }

  .chip:has(.chip-remove) {
    padding-right: 2px;
  }

  .chip:not(:has(.chip-remove)) {
    padding-right: 8px;
  }

  .chip-icon {
    display: inline-flex;
    align-items: center;
    color: var(--c-text-muted);
    flex-shrink: 0;
  }

  .chip.accent .chip-icon {
    color: var(--c-accent);
  }

  .chip-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chip-remove {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--c-text-muted);
    cursor: pointer;
    flex-shrink: 0;
    transition:
      background 0.1s,
      color 0.1s;
  }

  .chip-remove:hover {
    background: var(--c-hover-strong);
    color: var(--c-text);
  }

  .chip-remove:focus-visible {
    outline: none;
    box-shadow: 0 0 0 2px var(--c-focus-ring);
  }
</style>
