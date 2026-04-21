<script lang="ts">
  import type { Snippet } from 'svelte'
  import Tooltip from '../../shared/Tooltip.svelte'

  type Size = 'sm' | 'md'
  type Variant = 'ghost' | 'primary' | 'danger'

  interface Props {
    onclick?: (e: MouseEvent) => void
    tooltip?: string
    label?: string
    disabled?: boolean
    size?: Size
    variant?: Variant
    children: Snippet
  }

  let {
    onclick,
    tooltip,
    label,
    disabled = false,
    size = 'md',
    variant = 'ghost',
    children,
  }: Props = $props()

  let ariaLabel = $derived(label ?? tooltip)
</script>

{#snippet button()}
  <button
    class="icon-button {variant} {size}"
    type="button"
    {disabled}
    aria-label={ariaLabel}
    onclick={(e) => onclick?.(e)}
  >
    {@render children()}
  </button>
{/snippet}

{#if tooltip}
  <Tooltip text={tooltip}>
    {@render button()}
  </Tooltip>
{:else}
  {@render button()}
{/if}

<style>
  .icon-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 1px solid transparent;
    border-radius: 4px;
    background: transparent;
    color: var(--c-text-secondary);
    cursor: pointer;
    padding: 0;
    outline: none;
    transition:
      background 0.1s,
      color 0.1s,
      border-color 0.1s;
  }

  .icon-button.sm {
    width: 22px;
    height: 22px;
  }

  .icon-button.md {
    width: 26px;
    height: 26px;
  }

  .icon-button:hover:not(:disabled) {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .icon-button:active:not(:disabled) {
    background: var(--c-active);
  }

  .icon-button:focus-visible {
    border-color: var(--c-focus-ring);
    box-shadow: 0 0 0 2px var(--c-focus-ring);
  }

  .icon-button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .icon-button.primary {
    background: var(--c-accent);
    color: var(--c-bg);
  }

  .icon-button.primary:hover:not(:disabled) {
    background: var(--c-accent-text);
    color: var(--c-bg);
  }

  .icon-button.danger {
    background: var(--c-danger-bg);
    color: var(--c-danger-text);
    border-color: color-mix(in srgb, var(--c-danger) 40%, transparent);
  }

  .icon-button.danger:hover:not(:disabled) {
    background: var(--c-danger);
    color: var(--c-bg);
    border-color: var(--c-danger);
  }
</style>
