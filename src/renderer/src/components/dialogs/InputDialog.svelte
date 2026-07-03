<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import type { PromptCheckbox, PromptResult } from '../../lib/stores/dialogs.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'

  let {
    title,
    placeholder = '',
    initialValue = '',
    multiline = false,
    submitLabel = 'Submit',
    validate,
    onGenerate,
    checkbox,
    onSubmit,
    onCancel,
  }: {
    title: string
    placeholder?: string
    initialValue?: string
    multiline?: boolean
    submitLabel?: string
    validate?: (value: string) => string | null
    onGenerate?: () => Promise<string | null>
    checkbox?: PromptCheckbox
    onSubmit: (result: PromptResult) => void
    onCancel: () => void
  } = $props()

  const isMac = navigator.userAgent.includes('Mac')
  let value = $state(untrack(() => initialValue))
  let checked = $state(untrack(() => checkbox?.checked ?? false))
  let error = $derived(validate ? validate(value) : null)
  let generating = $state(false)
  let inputEl: HTMLInputElement | undefined = $state()
  let textareaEl: HTMLTextAreaElement | undefined = $state()
  let containerEl: HTMLDivElement | undefined = $state()

  onMount(() => {
    // Restore focus to the element that opened the dialog when it closes.
    const previouslyFocused = document.activeElement as HTMLElement | null
    if (multiline) {
      textareaEl?.focus()
    } else {
      inputEl?.focus()
    }
    return () => previouslyFocused?.focus?.()
  })

  function trySubmit(): void {
    const trimmed = value.trim()
    if (!trimmed) return
    if (error) return
    onSubmit({ value: trimmed, checked })
  }

  async function handleGenerate(): Promise<void> {
    if (!onGenerate || generating) return
    generating = true
    try {
      const msg = await onGenerate()
      if (msg) value = msg
    } catch {
      // Silently ignore — user can type manually
    } finally {
      generating = false
    }
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      onCancel()
      return
    }

    if (e.key === 'Tab' && containerEl) {
      const focusable = containerEl.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
      )
      if (focusable.length === 0) return
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      const active = document.activeElement as HTMLElement | null
      if (e.shiftKey && (active === first || !containerEl.contains(active))) {
        e.preventDefault()
        last.focus()
      } else if (!e.shiftKey && active === last) {
        e.preventDefault()
        first.focus()
      }
      return
    }

    if (e.key === 'Enter') {
      if (multiline) {
        const mod = isMac ? e.metaKey : e.ctrlKey
        if (mod) {
          e.preventDefault()
          trySubmit()
        }
      } else {
        e.preventDefault()
        trySubmit()
      }
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-overlay flex justify-center items-start pt-30 bg-scrim"
  onkeydown={handleKeydown}
  onmousedown={onCancel}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={containerEl}
    class="w-105 bg-bg-overlay border border-border rounded-2xl shadow-modal px-6 py-5 flex flex-col gap-4"
    role="dialog"
    aria-modal="true"
    aria-labelledby="input-dialog-title"
    onmousedown={(e) => e.stopPropagation()}
  >
    <h3 id="input-dialog-title" class="m-0 text-base font-semibold text-text">{title}</h3>

    <div class="flex flex-col gap-2">
      {#if multiline}
        <textarea
          bind:this={textareaEl}
          bind:value
          class="w-full border border-border rounded-md bg-bg-input text-text text-md font-inherit px-3 py-2 outline-none transition-colors duration-fast box-border resize-y min-h-20 focus:border-focus-ring placeholder:text-text-faint"
          {placeholder}
          rows="4"
          spellcheck="false"
          aria-invalid={!!error}
          aria-describedby={error ? 'input-dialog-error' : undefined}></textarea>
      {:else}
        <input
          bind:this={inputEl}
          bind:value
          class="w-full h-9 border border-border rounded-md bg-bg-input text-text text-md font-inherit px-3 outline-none transition-colors duration-fast box-border focus:border-focus-ring placeholder:text-text-faint"
          type="text"
          {placeholder}
          spellcheck="false"
          autocomplete="off"
          aria-invalid={!!error}
          aria-describedby={error ? 'input-dialog-error' : undefined}
        />
      {/if}

      {#if error}
        <p id="input-dialog-error" role="alert" class="m-0 text-sm text-danger-text">{error}</p>
      {/if}

      {#if checkbox}
        <label
          class="flex items-center gap-1.5 mt-1 text-sm text-text-secondary cursor-pointer select-none"
        >
          <CustomCheckbox {checked} onchange={(v) => (checked = v)} />
          <span>{checkbox.label}</span>
        </label>
      {/if}

      {#if multiline}
        <p class="m-0 text-xs text-text-faint">{isMac ? 'Cmd' : 'Ctrl'}+Enter to submit</p>
      {/if}
    </div>

    <div class="flex items-center gap-2">
      {#if onGenerate}
        <button
          class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none transition-colors duration-fast bg-generate-bg text-generate enabled:hover:bg-generate-bg-hover disabled:opacity-40 disabled:cursor-default focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
          onclick={handleGenerate}
          disabled={generating}
        >
          {generating ? 'Generating...' : 'AI Generate'}
        </button>
      {/if}
      <div class="flex gap-2 ml-auto">
        <button
          class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none transition-colors duration-fast bg-active text-text hover:bg-border focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
          onclick={onCancel}>Cancel</button
        >
        <button
          class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none transition-colors duration-fast bg-accent-bg text-accent-text enabled:hover:bg-accent-muted disabled:opacity-40 disabled:cursor-default focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
          onclick={trySubmit}
          disabled={!value.trim() || !!error}
        >
          {submitLabel}
        </button>
      </div>
    </div>
  </div>
</div>
