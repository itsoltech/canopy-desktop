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
    attachments?: Snippet
    modelPicker?: Snippet
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
    attachments,
    modelPicker,
    commandHints,
  }: Props = $props()

  let textareaEl: HTMLTextAreaElement | undefined = $state()

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

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Enter') {
      const shouldSubmit = e.metaKey || e.ctrlKey || !e.shiftKey
      if (shouldSubmit) {
        e.preventDefault()
        submit()
      }
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
  {#if commandHints}
    <div class="hints-slot">
      {@render commandHints()}
    </div>
  {/if}

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
    </div>
    <div class="right">
      <SendControl onsend={submit} disabled={!canSend} />
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
    border-radius: 10px;
    background: var(--c-bg-input);
    transition: border-color 0.12s ease;
  }

  .chat-input:focus-within {
    border-color: var(--c-focus-ring);
    box-shadow: 0 0 0 2px var(--c-focus-ring);
  }

  .chat-input.disabled {
    opacity: 0.6;
  }

  .hints-slot {
    border-bottom: 1px solid var(--c-border-subtle);
    padding: 4px;
  }

  .input-row {
    display: flex;
    padding: 10px 12px 6px;
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
    font-size: 13.5px;
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
    padding: 4px 8px 6px;
  }

  .left,
  .right {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
</style>
