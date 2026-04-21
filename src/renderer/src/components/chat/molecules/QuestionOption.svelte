<script lang="ts">
  import type { Snippet } from 'svelte'
  import CustomRadio from '../../shared/CustomRadio.svelte'
  import CustomCheckbox from '../../shared/CustomCheckbox.svelte'

  interface Props {
    label: string
    description?: string
    selected?: boolean
    multiSelect?: boolean
    disabled?: boolean
    /** True when a preview pane is rendered next to the options list in the parent. */
    hasPreview?: boolean
    /** Optional inline preview rendered below the row (used when options are stacked, no side-by-side layout). */
    preview?: Snippet
    onselect?: () => void
  }

  let {
    label,
    description,
    selected = false,
    multiSelect = false,
    disabled = false,
    hasPreview = false,
    preview,
    onselect,
  }: Props = $props()

  // "(Recommended)" convention from AskUserQuestion tool — strip it from the label
  // and render it as a distinct badge so it's visually consistent across options.
  let isRecommended = $derived(/\s*\(Recommended\)\s*$/i.test(label))
  let cleanLabel = $derived(label.replace(/\s*\(Recommended\)\s*$/i, ''))

  function handleClick(): void {
    if (disabled) return
    onselect?.()
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (disabled) return
    if (e.key === ' ' || e.key === 'Enter') {
      e.preventDefault()
      onselect?.()
    }
  }
</script>

<div
  class="option"
  class:selected
  class:disabled
  class:focused={!disabled}
  role={multiSelect ? 'checkbox' : 'radio'}
  aria-checked={selected}
  aria-disabled={disabled}
  tabindex={disabled ? -1 : 0}
  onclick={handleClick}
  onkeydown={handleKeydown}
>
  <span class="indicator" aria-hidden="true">
    {#if multiSelect}
      <CustomCheckbox checked={selected} {disabled} />
    {:else}
      <CustomRadio checked={selected} {disabled} />
    {/if}
  </span>

  <div class="body">
    <div class="label-row">
      <span class="label">{cleanLabel}</span>
      {#if isRecommended}
        <span class="recommended-badge">Recommended</span>
      {/if}
      {#if hasPreview}
        <span class="preview-marker" title="Has preview">◨</span>
      {/if}
    </div>
    {#if description}
      <div class="description">{description}</div>
    {/if}
    {#if preview}
      <div class="inline-preview">{@render preview()}</div>
    {/if}
  </div>
</div>

<style>
  .option {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 8px 10px;
    border: 1px solid var(--c-border-subtle);
    border-radius: 6px;
    background: transparent;
    cursor: pointer;
    outline: none;
    transition:
      background 0.1s,
      border-color 0.1s;
  }

  .option.focused:hover {
    background: var(--c-hover);
    border-color: var(--c-border);
  }

  .option:focus-visible {
    border-color: var(--c-focus-ring);
    box-shadow: 0 0 0 2px var(--c-focus-ring);
  }

  .option.selected {
    background: var(--c-accent-bg);
    border-color: color-mix(in srgb, var(--c-accent) 55%, transparent);
  }

  .option.disabled {
    cursor: default;
    opacity: 0.7;
  }

  .indicator {
    display: inline-flex;
    align-items: center;
    padding-top: 1px;
    /* Clicks pass through to the row — the visual widget is non-interactive. */
    pointer-events: none;
    flex-shrink: 0;
  }

  .body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .label-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .label {
    font-size: 12.5px;
    color: var(--c-text);
    font-weight: 500;
    line-height: 1.3;
  }

  .recommended-badge {
    display: inline-flex;
    align-items: center;
    height: 16px;
    padding: 0 6px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--c-success) 18%, transparent);
    border: 1px solid color-mix(in srgb, var(--c-success) 40%, transparent);
    color: var(--c-success);
    font-size: 9.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .preview-marker {
    color: var(--c-text-muted);
    font-size: 11px;
    line-height: 1;
  }

  .description {
    font-size: 11.5px;
    color: var(--c-text-muted);
    line-height: 1.45;
  }

  .inline-preview {
    margin-top: 6px;
    border: 1px solid var(--c-border-subtle);
    border-radius: 4px;
    background: var(--c-bg);
    padding: 8px 10px;
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 11px;
    line-height: 1.45;
    white-space: pre;
    overflow-x: auto;
    color: var(--c-text);
  }
</style>
