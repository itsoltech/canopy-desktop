<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import type { PromptCheckbox, PromptResult } from '../../lib/stores/dialogs.svelte'

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

  onMount(() => {
    if (multiline) {
      textareaEl?.focus()
    } else {
      inputEl?.focus()
    }
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

    if (e.key === 'Tab') {
      const container = e.currentTarget as HTMLElement
      const focusable = container.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
      )
      if (focusable.length === 0) return
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault()
        last.focus()
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault()
        first.focus()
      }
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="dialog-overlay" onkeydown={handleKeydown} onclick={onCancel}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="dialog-container"
    role="dialog"
    aria-modal="true"
    aria-labelledby="input-dialog-title"
    onclick={(e) => e.stopPropagation()}
  >
    <h3 id="input-dialog-title" class="dialog-title">{title}</h3>

    {#if multiline}
      <textarea
        bind:this={textareaEl}
        bind:value
        class="dialog-textarea"
        {placeholder}
        rows="4"
        spellcheck="false"
      ></textarea>
    {:else}
      <input
        bind:this={inputEl}
        bind:value
        class="dialog-input"
        type="text"
        {placeholder}
        spellcheck="false"
        autocomplete="off"
      />
    {/if}

    {#if error}
      <p class="dialog-error">{error}</p>
    {/if}

    {#if checkbox}
      <label class="dialog-checkbox">
        <input type="checkbox" bind:checked />
        <span>{checkbox.label}</span>
      </label>
    {/if}

    {#if multiline}
      <p class="dialog-hint">{isMac ? 'Cmd' : 'Ctrl'}+Enter to submit</p>
    {/if}

    <div class="dialog-actions">
      {#if onGenerate}
        <button class="btn btn-generate" onclick={handleGenerate} disabled={generating}>
          {generating ? 'Generating...' : 'AI Generate'}
        </button>
      {/if}
      <div class="dialog-actions-right">
        <button class="btn btn-cancel" onclick={onCancel}>Cancel</button>
        <button class="btn btn-submit" onclick={trySubmit} disabled={!value.trim() || !!error}>
          {submitLabel}
        </button>
      </div>
    </div>
  </div>
</div>

<style>
  .dialog-overlay {
    position: fixed;
    inset: 0;
    z-index: 1001;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 120px;
    background: rgba(0, 0, 0, 0.5);
  }

  .dialog-container {
    width: 420px;
    background: rgba(30, 30, 30, 0.98);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 10px;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.6);
    padding: 20px;
  }

  .dialog-title {
    margin: 0 0 12px;
    font-size: 15px;
    font-weight: 600;
    color: #e0e0e0;
  }

  .dialog-input,
  .dialog-textarea {
    width: 100%;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.3);
    color: #e0e0e0;
    font-size: 13px;
    font-family: inherit;
    padding: 8px 10px;
    outline: none;
    transition: border-color 0.1s;
    box-sizing: border-box;
  }

  .dialog-input:focus,
  .dialog-textarea:focus {
    border-color: rgba(116, 192, 252, 0.5);
  }

  .dialog-textarea {
    resize: vertical;
    min-height: 80px;
  }

  .dialog-input::placeholder,
  .dialog-textarea::placeholder {
    color: rgba(255, 255, 255, 0.25);
  }

  .dialog-error {
    margin: 6px 0 0;
    font-size: 12px;
    color: rgba(255, 120, 120, 0.9);
  }

  .dialog-checkbox {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 10px 0 0;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.6);
    cursor: pointer;
    user-select: none;
  }

  .dialog-checkbox input {
    accent-color: rgba(116, 192, 252, 0.8);
    cursor: pointer;
  }

  .dialog-hint {
    margin: 6px 0 0;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.25);
  }

  .dialog-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 16px;
  }

  .dialog-actions-right {
    display: flex;
    gap: 8px;
    margin-left: auto;
  }

  .btn {
    padding: 6px 14px;
    border-radius: 6px;
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
    border: none;
    outline: none;
    transition: background 0.1s;
  }

  .btn:focus-visible {
    outline: 2px solid rgba(116, 192, 252, 0.6);
    outline-offset: 1px;
  }

  .btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .btn-cancel {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.7);
  }

  .btn-cancel:hover {
    background: rgba(255, 255, 255, 0.12);
  }

  .btn-submit {
    background: rgba(116, 192, 252, 0.2);
    color: rgba(116, 192, 252, 0.9);
  }

  .btn-submit:hover:not(:disabled) {
    background: rgba(116, 192, 252, 0.3);
  }

  .btn-generate {
    background: rgba(168, 130, 255, 0.2);
    color: rgba(168, 130, 255, 0.9);
  }

  .btn-generate:hover:not(:disabled) {
    background: rgba(168, 130, 255, 0.3);
  }
</style>
