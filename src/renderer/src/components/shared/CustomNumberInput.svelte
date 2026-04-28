<script lang="ts">
  interface Props {
    value: string | number
    min?: number
    max?: number
    step?: number
    onchange?: (value: string) => void
    id?: string
  }

  let { value, min = 0, max = 100, step = 1, onchange, id }: Props = $props()

  let editing = $state(false)
  let editValue = $state('')
  let displayValue = $derived(String(value))
  let repeatTimer: ReturnType<typeof setTimeout> | null = null
  let repeatInterval: ReturnType<typeof setInterval> | null = null

  function clamp(n: number): number {
    return Math.min(max, Math.max(min, n))
  }

  function commit(val: string): void {
    editing = false
    const n = parseInt(val, 10)
    if (isNaN(n)) return
    onchange?.(String(clamp(n)))
  }

  function increment(): void {
    const n = clamp(parseInt(String(value), 10) || 0)
    const next = clamp(n + step)
    editing = false
    onchange?.(String(next))
  }

  function decrement(): void {
    const n = clamp(parseInt(String(value), 10) || 0)
    const next = clamp(n - step)
    editing = false
    onchange?.(String(next))
  }

  function startRepeat(fn: () => void): void {
    fn()
    repeatTimer = setTimeout(() => {
      repeatInterval = setInterval(fn, 100)
    }, 400)
  }

  function stopRepeat(): void {
    if (repeatTimer) {
      clearTimeout(repeatTimer)
      repeatTimer = null
    }
    if (repeatInterval) {
      clearInterval(repeatInterval)
      repeatInterval = null
    }
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'ArrowUp') {
      e.preventDefault()
      increment()
    } else if (e.key === 'ArrowDown') {
      e.preventDefault()
      decrement()
    } else if (e.key === 'Enter') {
      e.preventDefault()
      commit(editValue)
    }
  }
</script>

<div
  class="inline-flex items-center w-fit border border-border rounded-lg bg-hover overflow-hidden focus-within:border-focus-ring"
>
  <button
    class="flex items-center justify-center w-7 py-1.5 border-0 border-r border-active bg-transparent text-text-secondary text-lg font-inherit cursor-pointer select-none transition-colors duration-fast hover:bg-hover-strong hover:text-text"
    tabindex="-1"
    aria-label="Decrease"
    onpointerdown={() => startRepeat(decrement)}
    onpointerup={stopRepeat}
    onpointerleave={stopRepeat}
  >
    &minus;
  </button>
  <input
    {id}
    class="w-10 text-center border-0 bg-transparent text-text text-md font-inherit outline-none py-1.5"
    type="text"
    inputmode="numeric"
    value={editing ? editValue : displayValue}
    onfocus={(e) => {
      editing = true
      editValue = e.currentTarget.value
    }}
    oninput={(e) => (editValue = e.currentTarget.value)}
    onblur={() => commit(editValue)}
    onkeydown={handleKeydown}
  />
  <button
    class="flex items-center justify-center w-7 py-1.5 border-0 border-l border-active bg-transparent text-text-secondary text-lg font-inherit cursor-pointer select-none transition-colors duration-fast hover:bg-hover-strong hover:text-text"
    tabindex="-1"
    aria-label="Increase"
    onpointerdown={() => startRepeat(increment)}
    onpointerup={stopRepeat}
    onpointerleave={stopRepeat}
  >
    +
  </button>
</div>
