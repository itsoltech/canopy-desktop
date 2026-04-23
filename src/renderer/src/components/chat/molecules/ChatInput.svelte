<script lang="ts">
  import type { Snippet } from 'svelte'
  import { Paperclip } from '@lucide/svelte'
  import IconButton from '../atoms/IconButton.svelte'
  import SendControl from './SendControl.svelte'

  interface Props {
    value?: string
    placeholder?: string
    disabled?: boolean
    maxRows?: number
    onsubmit?: (value: string) => void
    onchange?: (value: string) => void
    onattach?: () => void
    onstop?: () => void
    /** Optional parent pre-handler. Returning true halts default key behavior. */
    onKeydownIntercept?: (event: KeyboardEvent) => boolean
    /** Emits the textarea element once mounted for imperative caret/focus access. */
    onTextareaReady?: (el: HTMLTextAreaElement) => void
    attachments?: Snippet
    modelPicker?: Snippet
    permissionMode?: Snippet
    effortPicker?: Snippet
    commandHints?: Snippet
  }

  let {
    value = $bindable(''),
    placeholder = 'Message…',
    disabled = false,
    maxRows = 10,
    onsubmit,
    onchange,
    onattach,
    onstop,
    onKeydownIntercept,
    onTextareaReady,
    attachments,
    modelPicker,
    permissionMode,
    effortPicker,
  }: Props = $props()

  let textareaEl: HTMLTextAreaElement | undefined = $state()

  $effect(() => {
    if (textareaEl) onTextareaReady?.(textareaEl)
  })

  let canSend = $derived(!disabled && value.trim().length > 0)

  function autoResize(): void {
    if (!textareaEl) return
    textareaEl.style.height = 'auto'
    const lineHeight = 20
    const maxHeight = lineHeight * maxRows + 16
    textareaEl.style.height = `${Math.min(textareaEl.scrollHeight, maxHeight)}px`
  }

  function submit(): void {
    if (!canSend) return
    const text = value.trim()
    onsubmit?.(text)
    value = ''
    queueMicrotask(autoResize)
  }

  function insertAtCaret(text: string): void {
    if (!textareaEl) {
      value = `${value}${text}`
      return
    }
    const start = textareaEl.selectionStart ?? value.length
    const end = textareaEl.selectionEnd ?? value.length
    const next = value.slice(0, start) + text + value.slice(end)
    value = next
    onchange?.(value)
    queueMicrotask(() => {
      if (!textareaEl) return
      const caret = start + text.length
      textareaEl.focus()
      textareaEl.setSelectionRange(caret, caret)
      autoResize()
    })
  }

  function clearValue(): void {
    value = ''
    onchange?.(value)
    queueMicrotask(() => {
      textareaEl?.focus()
      autoResize()
    })
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (onKeydownIntercept?.(e)) return
    if (e.key === 'Enter') {
      const shouldSubmit = e.metaKey || e.ctrlKey || !e.shiftKey
      if (shouldSubmit) {
        e.preventDefault()
        submit()
      }
      return
    }
    // Ctrl+J → insert newline (readline-style, works in any terminal).
    if (e.key === 'j' && e.ctrlKey && !e.metaKey && !e.shiftKey && !e.altKey) {
      e.preventDefault()
      insertAtCaret('\n')
      return
    }
    // Cmd/Ctrl+L → clear input (conversation kept).
    if (e.key === 'l' && (e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey) {
      e.preventDefault()
      clearValue()
    }
  }

  function handleInput(e: Event): void {
    const target = e.currentTarget as HTMLTextAreaElement
    value = target.value
    onchange?.(value)
    autoResize()
  }

  $effect(() => {
    // Keep height in sync when value is updated externally.
    void value
    autoResize()
  })
</script>

<div class="chat-input" class:disabled>
  {#if attachments}
    {@render attachments()}
  {/if}

  <div class="input-row">
    <textarea
      bind:this={textareaEl}
      class="textarea"
      rows="1"
      {placeholder}
      {disabled}
      {value}
      oninput={handleInput}
      onkeydown={handleKeydown}
    ></textarea>
  </div>

  <div class="toolbar">
    <div class="left">
      {#if onattach}
        <IconButton
          onclick={() => onattach?.()}
          tooltip="Attach file"
          label="Attach file"
          size="sm"
        >
          <Paperclip size={14} />
        </IconButton>
      {/if}
      {#if modelPicker}
        {@render modelPicker()}
      {/if}
      {#if permissionMode}
        {@render permissionMode()}
      {/if}
      {#if effortPicker}
        {@render effortPicker()}
      {/if}
    </div>
    <div class="right">
      <SendControl onsend={submit} {onstop} disabled={!canSend} stopping={disabled} />
    </div>
  </div>
</div>

<style>
  .chat-input {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--c-border);
    border-radius: 0;
    background: color-mix(in srgb, var(--c-bg) 88%, black);
    transition: border-color 0.12s ease;
    font-family: inherit;
    font-size: inherit;
  }

  .chat-input:focus-within {
    border-color: var(--c-focus-ring);
    box-shadow: inset 0 0 0 1px var(--c-focus-ring);
  }

  .chat-input.disabled {
    opacity: 0.6;
  }

  .input-row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 10px 5px;
  }

  .input-row::before {
    content: '>';
    color: var(--c-accent-text);
    line-height: 20px;
    flex-shrink: 0;
  }

  .textarea {
    flex: 1;
    width: 100%;
    min-height: 20px;
    max-height: 100%;
    padding: 0;
    border: none;
    outline: none;
    background: transparent;
    color: var(--c-text);
    font-family: inherit;
    font-size: inherit;
    line-height: 20px;
    resize: none;
    overflow-y: auto;
  }

  .textarea::placeholder {
    color: var(--c-text-muted);
  }

  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 4px 8px 6px 26px;
    border-top: 1px solid var(--c-border-subtle);
  }

  .left,
  .right {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
</style>
