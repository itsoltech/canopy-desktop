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

<div class="number-wrapper">
  <button
    class="spin-btn"
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
    class="number-field"
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
    class="spin-btn"
    tabindex="-1"
    aria-label="Increase"
    onpointerdown={() => startRepeat(increment)}
    onpointerup={stopRepeat}
    onpointerleave={stopRepeat}
  >
    +
  </button>
</div>

<style>
  .number-wrapper {
    display: inline-flex;
    align-items: center;
    width: fit-content;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-hover);
    overflow: hidden;
  }

  .number-wrapper:focus-within {
    border-color: var(--c-focus-ring);
  }

  .number-field {
    width: 40px;
    text-align: center;
    border: none;
    background: transparent;
    color: var(--c-text);
    font-size: 13px;
    font-family: inherit;
    outline: none;
    padding: 6px 0;
  }

  .spin-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    padding: 6px 0;
    border: none;
    background: transparent;
    color: var(--c-text-secondary);
    font-size: 14px;
    font-family: inherit;
    cursor: pointer;
    user-select: none;
    transition: background 0.1s;
  }

  .spin-btn:hover {
    background: var(--c-hover-strong);
    color: var(--c-text);
  }

  .spin-btn:first-child {
    border-right: 1px solid var(--c-active);
  }

  .spin-btn:last-child {
    border-left: 1px solid var(--c-active);
  }
</style>
