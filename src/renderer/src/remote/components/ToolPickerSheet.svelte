<script lang="ts">
  import { onMount } from 'svelte'

  interface Tool {
    id: string
    name: string
    available: boolean
  }

  let {
    tools,
    workspaceLabel,
    onClose,
    onPick,
  }: {
    tools: Tool[]
    workspaceLabel: string
    onClose: () => void
    onPick: (toolId: string) => void
  } = $props()

  let availableTools = $derived(tools.filter((t) => t.available))
  let sheetEl: HTMLElement | undefined = $state()

  // The sheet announces aria-modal, so it has to behave like one: pull focus in
  // on open, keep Tab inside, and hand focus back on close. Kept local rather
  // than importing lib/a11y/focusTrap — the remote app is a separate bundle and
  // does not reach into the main renderer's lib.
  onMount(() => {
    const previouslyFocused = document.activeElement as HTMLElement | null
    sheetEl?.querySelector<HTMLElement>('button')?.focus()
    return () => {
      if (previouslyFocused?.isConnected) previouslyFocused.focus()
    }
  })

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      onClose()
      return
    }
    if (e.key !== 'Tab' || !sheetEl) return
    const focusable = sheetEl.querySelectorAll<HTMLElement>(
      'button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])',
    )
    if (focusable.length === 0) return
    const first = focusable[0]
    const last = focusable[focusable.length - 1]
    const active = document.activeElement
    const outside = !sheetEl.contains(active)
    if (e.shiftKey && (active === first || outside)) {
      e.preventDefault()
      last.focus()
    } else if (!e.shiftKey && (active === last || outside)) {
      e.preventDefault()
      first.focus()
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<button
  type="button"
  class="sheet-backdrop fixed inset-0 z-[2500] cursor-pointer bg-scrim border-0 m-0 p-0 appearance-none"
  aria-label="Close tool picker"
  onclick={onClose}
></button>
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="tool-sheet fixed left-1/2 bottom-0 -translate-x-1/2 z-[2501] w-full max-w-[520px] max-h-[75dvh] bg-bg-elevated rounded-t-[14px] border border-border border-b-0 shadow-[0_-8px_24px_oklch(0_0_0/0.35)] flex flex-col"
  role="dialog"
  aria-modal="true"
  aria-labelledby="tool-sheet-title"
  bind:this={sheetEl}
  onclick={(e) => e.stopPropagation()}
>
  <header
    class="flex-shrink-0 flex items-center justify-between px-[18px] pt-3.5 pb-2.5 border-b border-border-subtle"
  >
    <span id="tool-sheet-title" class="text-lg font-semibold text-text">Spawn a tool</span>
    <button
      type="button"
      class="inline-flex items-center justify-center w-[30px] h-[30px] text-lg rounded-md cursor-pointer bg-bg-input text-text-secondary border-0 hover:bg-hover-strong"
      onclick={onClose}
      aria-label="Close"
    >
      ×
    </button>
  </header>
  <div class="sheet-body flex-1 overflow-y-auto px-3.5 pt-2.5 pb-[18px] flex flex-col gap-1.5">
    {#each availableTools as tool (tool.id)}
      <button
        type="button"
        class="flex flex-col items-start gap-0.5 p-3.5 bg-bg border border-border rounded-[10px] cursor-pointer text-left active:bg-accent-bg active:border-accent-muted"
        onclick={() => onPick(tool.id)}
      >
        <span class="text-[15px] font-semibold text-text">{tool.name}</span>
        <span class="text-xs text-text-muted">Spawn in {workspaceLabel || 'current worktree'}</span>
      </button>
    {/each}
    {#if availableTools.length === 0}
      <p class="m-0 text-sm text-text-muted leading-normal">No tools registered on host.</p>
    {/if}
  </div>
</div>
