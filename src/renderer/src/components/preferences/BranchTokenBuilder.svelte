<script lang="ts">
  interface TemplateToken {
    type: 'placeholder' | 'separator'
    value: string
  }

  let {
    templateInput = $bindable(''),
    placeholders,
    onSave,
    label = 'Template',
    autoSeparators = true,
  }: {
    templateInput: string
    placeholders: Array<{ key: string; description: string; example: string }>
    onSave: () => void
    label?: string
    autoSeparators?: boolean
  } = $props()

  const SEPARATORS = ['/', '-', '_']

  let templateTokens = $derived.by(() => parseTemplate(templateInput))
  let lastTokenIsSeparator = $derived(
    templateTokens.length > 0 && templateTokens[templateTokens.length - 1].type === 'separator',
  )

  // Drag state
  let dragIdx: number | null = $state(null)
  let dragOverIdx: number | null = $state(null)
  let dragFromAvailable: string | null = $state(null)

  // Popups
  let sepPopup = $state<{ visible: boolean; pendingKey: string; x: number; y: number }>({
    visible: false,
    pendingKey: '',
    x: 0,
    y: 0,
  })
  let editSepPopup = $state<{ visible: boolean; tokenIdx: number; x: number; y: number }>({
    visible: false,
    tokenIdx: -1,
    x: 0,
    y: 0,
  })

  function parseTemplate(tpl: string): TemplateToken[] {
    const tokens: TemplateToken[] = []
    const regex = /(\{[^}]+\})|([^{]+)/g
    let match: RegExpExecArray | null
    while ((match = regex.exec(tpl)) !== null) {
      if (match[1]) {
        tokens.push({ type: 'placeholder', value: match[1] })
      } else if (match[2]) {
        tokens.push({ type: 'separator', value: match[2] })
      }
    }
    return tokens
  }

  function tokensToTemplate(tokens: TemplateToken[]): string {
    return tokens.map((t) => t.value).join('')
  }

  function addPlaceholderToTemplate(key: string, e?: MouseEvent): void {
    const tag = `{${key}}`
    if (templateInput.includes(tag)) return

    if (!autoSeparators) {
      templateInput = templateInput ? templateInput + ' ' + tag : tag
      onSave()
    } else if (!templateInput || lastTokenIsSeparator) {
      templateInput = templateInput + tag
      onSave()
    } else {
      sepPopup = {
        visible: true,
        pendingKey: key,
        x: e?.clientX ?? 200,
        y: e?.clientY ?? 200,
      }
    }
  }

  function confirmSeparatorAndAdd(sep: string): void {
    const tag = `{${sepPopup.pendingKey}}`
    templateInput = templateInput + sep + tag
    sepPopup = { visible: false, pendingKey: '', x: 0, y: 0 }
    onSave()
  }

  function closeSepPopup(): void {
    sepPopup = { visible: false, pendingKey: '', x: 0, y: 0 }
  }

  function onSeparatorTokenClick(index: number, e: MouseEvent): void {
    editSepPopup = { visible: true, tokenIdx: index, x: e.clientX, y: e.clientY }
  }

  function changeSeparator(newSep: string): void {
    const tokens = [...templateTokens]
    tokens[editSepPopup.tokenIdx] = { type: 'separator', value: newSep }
    templateInput = tokensToTemplate(tokens)
    editSepPopup = { visible: false, tokenIdx: -1, x: 0, y: 0 }
    onSave()
  }

  function removeSeparatorToken(): void {
    removeTokenAt(editSepPopup.tokenIdx)
    editSepPopup = { visible: false, tokenIdx: -1, x: 0, y: 0 }
  }

  function closeEditSepPopup(): void {
    editSepPopup = { visible: false, tokenIdx: -1, x: 0, y: 0 }
  }

  function ensureSeparators(tokens: TemplateToken[]): TemplateToken[] {
    if (!autoSeparators) return tokens
    const result: TemplateToken[] = []
    for (let i = 0; i < tokens.length; i++) {
      result.push(tokens[i])
      if (tokens[i].type === 'placeholder' && tokens[i + 1]?.type === 'placeholder') {
        result.push({ type: 'separator', value: '/' })
      }
    }
    return result
  }

  function removeTokenAt(index: number): void {
    const tokens = [...templateTokens]
    tokens.splice(index, 1)
    templateInput = tokensToTemplate(tokens)
      .replace(/\/{2,}/g, '/')
      .replace(/-{2,}/g, '-')
      .replace(/_{2,}/g, '_')
      .replace(/^[/\-_]|[/\-_]$/g, '')
    onSave()
  }

  function onTokenDragStart(index: number): void {
    dragIdx = index
    dragFromAvailable = null
  }

  function onAvailableDragStart(key: string): void {
    dragFromAvailable = key
    dragIdx = null
  }

  function onTokenDragOver(index: number, e: DragEvent): void {
    e.preventDefault()
    dragOverIdx = index
  }

  function onTrackDragOver(e: DragEvent): void {
    e.preventDefault()
    if (dragFromAvailable) dragOverIdx = templateTokens.length
  }

  function onTokenDrop(index: number): void {
    if (dragFromAvailable) {
      const tag = `{${dragFromAvailable}}`
      if (!templateInput.includes(tag)) {
        const tokens = [...templateTokens]
        tokens.splice(index, 0, { type: 'placeholder', value: tag })
        templateInput = tokensToTemplate(ensureSeparators(tokens))
        onSave()
      }
      dragFromAvailable = null
      dragOverIdx = null
      return
    }
    if (dragIdx === null || dragIdx === index) {
      dragIdx = null
      dragOverIdx = null
      return
    }
    const tokens = [...templateTokens]
    const [moved] = tokens.splice(dragIdx, 1)
    tokens.splice(index, 0, moved)
    templateInput = tokensToTemplate(ensureSeparators(tokens))
    dragIdx = null
    dragOverIdx = null
    onSave()
  }

  function onTrackDrop(): void {
    if (dragFromAvailable) {
      const tag = `{${dragFromAvailable}}`
      if (!templateInput.includes(tag)) {
        const tokens = [...templateTokens, { type: 'placeholder' as const, value: tag }]
        templateInput = tokensToTemplate(ensureSeparators(tokens))
        onSave()
      }
      dragFromAvailable = null
      dragOverIdx = null
    }
  }

  function onTokenDragEnd(): void {
    dragIdx = null
    dragOverIdx = null
    dragFromAvailable = null
  }
</script>

<div class="token-builder">
  <label class="form-label">{label}</label>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="token-track" ondragover={onTrackDragOver} ondrop={onTrackDrop}>
    {#each templateTokens as token, i (`${token.type}:${token.value}:${i}`)}
      {#if token.type === 'placeholder'}
        <span
          class="token placeholder"
          class:drag-over={dragOverIdx === i}
          draggable="true"
          ondragstart={() => onTokenDragStart(i)}
          ondragover={(e) => onTokenDragOver(i, e)}
          ondrop={() => onTokenDrop(i)}
          ondragend={onTokenDragEnd}
          role="listitem"
        >
          {token.value}
          <button class="token-remove" onclick={() => removeTokenAt(i)} aria-label="Remove token"
            >×</button
          >
        </span>
      {:else}
        <button
          class="token separator"
          class:drag-over={dragOverIdx === i}
          draggable="true"
          ondragstart={() => onTokenDragStart(i)}
          ondragover={(e) => onTokenDragOver(i, e)}
          ondrop={() => onTokenDrop(i)}
          ondragend={onTokenDragEnd}
          onclick={(e) => onSeparatorTokenClick(i, e)}
        >
          {token.value}
        </button>
      {/if}
    {/each}
    {#if templateTokens.length === 0}
      <span class="token-empty">Drag or click tags below to build template</span>
    {/if}
  </div>
</div>

<div class="placeholder-list">
  <span class="placeholder-hint">Available tags: </span>
  {#each placeholders as ph (ph.key)}
    <button
      class="placeholder-tag"
      class:used={templateInput.includes('{' + ph.key + '}')}
      title={ph.description + ' (e.g. ' + ph.example + ')'}
      draggable="true"
      ondragstart={() => onAvailableDragStart(ph.key)}
      ondragend={onTokenDragEnd}
      onclick={(e) => addPlaceholderToTemplate(ph.key, e)}
    >
      &#123;{ph.key}&#125;
    </button>
  {/each}
</div>

<details class="advanced-template">
  <summary>Manual edit</summary>
  <input class="form-input" bind:value={templateInput} oninput={() => onSave()} />
  <p class="section-desc" style="margin-top: 6px;">
    Conditional: <code>{'{?parentKey}...{/parentKey}'}</code> — only renders if value exists.
  </p>
</details>

{#if sepPopup.visible}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="popup-overlay" onclick={closeSepPopup}>
    <div
      class="sep-popup"
      style="left:{sepPopup.x}px;top:{sepPopup.y}px"
      onclick={(e) => e.stopPropagation()}
    >
      <span class="popup-hint">Separator:</span>
      {#each SEPARATORS as sep (sep)}
        <button class="popup-sep-btn" onclick={() => confirmSeparatorAndAdd(sep)}>
          <code>{sep}</code>
        </button>
      {/each}
    </div>
  </div>
{/if}

{#if editSepPopup.visible}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="popup-overlay" onclick={closeEditSepPopup}>
    <div
      class="sep-popup"
      style="left:{editSepPopup.x}px;top:{editSepPopup.y}px"
      onclick={(e) => e.stopPropagation()}
    >
      <span class="popup-hint">Change to:</span>
      {#each SEPARATORS as sep (sep)}
        <button class="popup-sep-btn" onclick={() => changeSeparator(sep)}>
          <code>{sep}</code>
        </button>
      {/each}
      <button class="popup-sep-btn remove" onclick={removeSeparatorToken}>x</button>
    </div>
  </div>
{/if}

<style>
  .form-label {
    font-size: 12px;
    color: var(--color-text-secondary);
    width: 90px;
    flex-shrink: 0;
  }

  .form-input {
    flex: 1;
    padding: 5px 8px;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: var(--color-bg-input);
    color: var(--color-text);
    font-size: 12px;
    font-family: inherit;
    outline: none;
  }

  .form-input:focus {
    border-color: var(--color-focus-ring);
  }

  .section-desc {
    font-size: 12px;
    color: var(--color-text-muted);
    margin: 0 0 12px;
  }

  .token-builder {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }

  .token-track {
    display: flex;
    flex-wrap: wrap;
    gap: 3px;
    flex: 1;
    min-height: 28px;
    padding: 3px 6px;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: var(--color-bg-input);
    align-items: center;
    box-sizing: content-box;
  }

  .token-empty {
    font-size: 11px;
    color: var(--color-text-faint);
    padding: 2px 4px;
    line-height: 22px;
  }

  .token {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 2px 6px;
    border-radius: 4px;
    font-size: 11px;
    cursor: grab;
    user-select: none;
    transition: all 0.1s;
  }

  .token:active {
    cursor: grabbing;
  }

  .token.placeholder {
    background: var(--color-accent-bg);
    color: var(--color-accent-text);
    border: 1px solid var(--color-accent-muted);
  }

  .token.separator {
    background: var(--color-hover);
    color: var(--color-text-muted);
    border: 1px solid transparent;
    font-family: monospace;
  }

  .token.drag-over {
    border-color: var(--color-focus-ring);
    box-shadow: 0 0 0 1px var(--color-accent-muted);
  }

  .token-remove {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    border: none;
    border-radius: 50%;
    background: none;
    color: var(--color-text-muted);
    font-size: 12px;
    line-height: 1;
    cursor: pointer;
    padding: 0;
  }

  .token-remove:hover {
    background: var(--color-danger-bg);
    color: var(--color-danger-text);
  }

  .placeholder-list {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    margin-bottom: 4px;
  }

  .placeholder-hint {
    font-size: 11px;
    color: var(--color-text-faint);
  }

  .placeholder-tag {
    font-size: 11px;
    padding: 2px 7px;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-hover);
    color: var(--color-text-secondary);
    font-family: inherit;
    cursor: pointer;
    transition: all 0.1s;
  }

  .placeholder-tag:hover {
    background: var(--color-accent-bg);
    border-color: var(--color-accent-muted);
    color: var(--color-accent-text);
  }

  .placeholder-tag.used {
    opacity: 0.35;
    cursor: default;
  }

  .advanced-template {
    margin-top: 8px;
    margin-bottom: 8px;
  }

  .advanced-template summary {
    font-size: 11px;
    color: var(--color-text-faint);
    cursor: pointer;
    margin-bottom: 6px;
  }

  .advanced-template .form-input {
    width: 100%;
    box-sizing: border-box;
  }

  .popup-overlay {
    position: fixed;
    inset: 0;
    z-index: 1100;
  }

  .sep-popup {
    position: fixed;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    background: var(--color-bg-overlay);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    box-shadow: 0 4px 16px var(--color-scrim);
    transform: translate(-50%, -100%) translateY(-8px);
  }

  .popup-hint {
    font-size: 10px;
    color: var(--color-text-muted);
    margin-right: 2px;
  }

  .popup-sep-btn {
    padding: 3px 10px;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-hover);
    color: var(--color-text-secondary);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
  }

  .popup-sep-btn:hover {
    background: var(--color-accent-bg);
    border-color: var(--color-accent-muted);
    color: var(--color-accent-text);
  }

  .popup-sep-btn.remove {
    color: var(--color-danger-text);
  }

  .popup-sep-btn.remove:hover {
    background: var(--color-danger-bg);
    border-color: var(--color-danger-text);
    color: var(--color-danger-text);
  }
</style>
