<script lang="ts">
  interface Props {
    checked: boolean
    onchange?: () => void
    disabled?: boolean
  }

  let { checked, onchange, disabled = false }: Props = $props()

  function select(): void {
    if (disabled || checked) return
    onchange?.()
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === ' ' || e.key === 'Enter') {
      e.preventDefault()
      select()
    }
  }
</script>

<button
  class="radio"
  class:checked
  class:disabled
  role="radio"
  aria-checked={checked}
  aria-disabled={disabled}
  onclick={select}
  onkeydown={handleKeydown}
>
  {#if checked}
    <span class="dot"></span>
  {/if}
</button>

<style>
  .radio {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    min-width: 16px;
    border: 1.5px solid var(--c-text-muted);
    border-radius: 50%;
    background: transparent;
    cursor: pointer;
    outline: none;
    padding: 0;
    transition:
      background 0.1s,
      border-color 0.1s;
  }

  .radio:hover:not(.disabled) {
    border-color: var(--c-accent);
  }

  .radio:focus-visible {
    border-color: var(--c-focus-ring);
    box-shadow: 0 0 0 2px var(--c-focus-ring);
  }

  .radio.checked {
    border-color: var(--c-accent);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--c-accent);
  }

  .radio.disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
