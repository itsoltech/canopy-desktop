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
  class="inline-flex items-center justify-center w-4 h-4 min-w-4 border-1.5 rounded-md p-0 outline-none cursor-pointer transition-colors duration-fast focus-visible:ring-2 focus-visible:ring-focus-ring focus-visible:border-focus-ring"
  class:border-text-muted={!checked}
  class:bg-transparent={!checked}
  class:text-transparent={!checked}
  class:border-accent={checked}
  class:bg-accent={checked}
  class:text-bg={checked}
  class:cursor-default={disabled}
  class:opacity-40={disabled}
  class:hover:border-accent={!disabled && !checked}
  class:hover:border-accent-text={!disabled && checked}
  class:hover:bg-accent-text={!disabled && checked}
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
