<script lang="ts">
  interface Props {
    checked: boolean
    onchange?: () => void
    disabled?: boolean
    id?: string
    ariaLabel?: string
    ariaLabelledby?: string
    ariaDescribedby?: string
  }

  let {
    checked,
    onchange,
    disabled = false,
    id,
    ariaLabel,
    ariaLabelledby,
    ariaDescribedby,
  }: Props = $props()

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
  class="inline-flex items-center justify-center w-4 h-4 min-w-4 border-1.5 rounded-full bg-transparent p-0 outline-none cursor-pointer transition-colors duration-fast focus-visible:ring-2 focus-visible:ring-focus-ring focus-visible:border-focus-ring"
  class:border-text-muted={!checked}
  class:border-accent={checked}
  class:cursor-default={disabled}
  class:opacity-40={disabled}
  class:hover:border-accent={!disabled && !checked}
  {id}
  type="button"
  role="radio"
  aria-checked={checked}
  aria-disabled={disabled}
  aria-label={ariaLabel}
  aria-labelledby={ariaLabelledby}
  aria-describedby={ariaDescribedby}
  {disabled}
  onclick={select}
  onkeydown={handleKeydown}
>
  {#if checked}
    <span class="w-2 h-2 rounded-full bg-accent"></span>
  {/if}
</button>
