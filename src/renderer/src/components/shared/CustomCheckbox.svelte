<script lang="ts">
  import { Check } from '@lucide/svelte'

  interface Props {
    checked: boolean
    onchange?: (checked: boolean) => void
    disabled?: boolean
  }

  let { checked, onchange, disabled = false }: Props = $props()

  function toggle(): void {
    if (disabled) return
    onchange?.(!checked)
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === ' ' || e.key === 'Enter') {
      e.preventDefault()
      toggle()
    }
  }
</script>

<button
  class="checkbox"
  class:checked
  class:disabled
  role="checkbox"
  aria-checked={checked}
  aria-disabled={disabled}
  onclick={toggle}
  onkeydown={handleKeydown}
>
  {#if checked}
    <Check size={12} strokeWidth={3} />
  {/if}
</button>

<style>
  .checkbox {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    min-width: 16px;
    border: 1.5px solid var(--c-text-muted);
    border-radius: 4px;
    background: transparent;
    color: transparent;
    cursor: pointer;
    outline: none;
    padding: 0;
    transition:
      background 0.1s,
      border-color 0.1s,
      color 0.1s;
  }

  .checkbox:hover:not(.disabled) {
    border-color: var(--c-accent);
  }

  .checkbox:focus-visible {
    border-color: var(--c-focus-ring);
    box-shadow: 0 0 0 2px var(--c-focus-ring);
  }

  .checkbox.checked {
    background: var(--c-accent);
    border-color: var(--c-accent);
    color: var(--c-bg);
  }

  .checkbox.checked:hover:not(.disabled) {
    background: var(--c-accent-text);
    border-color: var(--c-accent-text);
  }

  .checkbox.disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
