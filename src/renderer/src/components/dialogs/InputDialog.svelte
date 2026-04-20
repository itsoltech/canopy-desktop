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
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="dialog-overlay" onkeydown={handleKeydown} onmousedown={onCancel}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="dialog-container"
    role="dialog"
    aria-modal="true"
    aria-labelledby="input-dialog-title"
    onmousedown={(e) => e.stopPropagation()}
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
        <CustomCheckbox {checked} onchange={(v) => (checked = v)} />
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
    background: var(--c-scrim);
  }

  .dialog-container {
    width: 420px;
    background: var(--c-bg-overlay);
    border: 1px solid var(--c-border);
    border-radius: 10px;
    box-shadow: var(--shadow-modal);
    padding: 20px;
  }

  .dialog-title {
    margin: 0 0 12px;
    font-size: 15px;
    font-weight: 600;
    color: var(--c-text);
  }

  .dialog-input,
  .dialog-textarea {
    width: 100%;
    border: 1px solid var(--c-border);
    border-radius: var(--r-md);
    background: var(--c-bg-input);
    color: var(--c-text);
    font-size: 13px;
    font-family: inherit;
    padding: 6px 10px;
    outline: none;
    transition: border-color var(--dur-fast);
    box-sizing: border-box;
  }

  .dialog-input:focus,
  .dialog-textarea:focus {
    border-color: var(--c-focus-ring);
  }

  .dialog-textarea {
    resize: vertical;
    min-height: 80px;
  }

  .dialog-input::placeholder,
  .dialog-textarea::placeholder {
    color: var(--c-text-faint);
  }

  .dialog-error {
    margin: 6px 0 0;
    font-size: 12px;
    color: var(--c-danger-text);
  }

  .dialog-checkbox {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 10px 0 0;
    font-size: 12px;
    color: var(--c-text-secondary);
    cursor: pointer;
    user-select: none;
  }

  .dialog-hint {
    margin: 6px 0 0;
    font-size: 11px;
    color: var(--c-text-faint);
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
    transition: background var(--dur-fast);
  }

  .btn:focus-visible {
    outline: 2px solid var(--c-focus-ring);
    outline-offset: 1px;
  }

  .btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .btn-cancel {
    background: var(--c-active);
    color: var(--c-text);
  }

  .btn-cancel:hover {
    background: var(--c-border);
  }

  .btn-submit {
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
  }

  .btn-submit:hover:not(:disabled) {
    background: var(--c-accent-muted);
  }

  .btn-generate {
    background: color-mix(in srgb, var(--c-generate) 20%, transparent);
    color: var(--c-generate);
  }

  .btn-generate:hover:not(:disabled) {
    background: color-mix(in srgb, var(--c-generate) 30%, transparent);
  }
</style>
