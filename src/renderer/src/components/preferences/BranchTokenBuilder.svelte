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

  let dragIdx: number | null = $state(null)
  let dragOverIdx: number | null = $state(null)
  let dragFromAvailable: string | null = $state(null)

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

<div class="flex items-center gap-2 mb-2">
  <label class="text-sm text-text-secondary w-[90px] flex-shrink-0">{label}</label>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="flex flex-wrap gap-[3px] flex-1 min-h-7 px-1.5 py-[3px] border border-border rounded-lg bg-bg-input items-center box-content"
    ondragover={onTrackDragOver}
    ondrop={onTrackDrop}
  >
    {#each templateTokens as token, i (`${token.type}:${token.value}:${i}`)}
      {#if token.type === 'placeholder'}
        <span
          class="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded-md text-xs cursor-grab select-none transition-all duration-fast bg-accent-bg text-accent-text border active:cursor-grabbing {dragOverIdx ===
          i
            ? 'border-focus-ring shadow-[0_0_0_1px_var(--color-accent-muted)]'
            : 'border-accent-muted'}"
          draggable="true"
          ondragstart={() => onTokenDragStart(i)}
          ondragover={(e) => onTokenDragOver(i, e)}
          ondrop={() => onTokenDrop(i)}
          ondragend={onTokenDragEnd}
          role="listitem"
        >
          {token.value}
          <button
            class="inline-flex items-center justify-center w-[14px] h-[14px] border-0 rounded-full bg-transparent text-text-muted text-sm leading-none cursor-pointer p-0 hover:bg-danger-bg hover:text-danger-text"
            onclick={() => removeTokenAt(i)}
            aria-label="Remove token">×</button
          >
        </span>
      {:else}
        <button
          class="inline-flex items-center px-1.5 py-0.5 rounded-md text-xs cursor-grab select-none transition-all duration-fast bg-hover text-text-muted border font-mono active:cursor-grabbing {dragOverIdx ===
          i
            ? 'border-focus-ring shadow-[0_0_0_1px_var(--color-accent-muted)]'
            : 'border-transparent'}"
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
      <span class="text-xs text-text-faint px-1 py-0.5 leading-[22px]"
        >Drag or click tags below to build template</span
      >
    {/if}
  </div>
</div>

<div class="flex flex-wrap items-center gap-1 mb-1">
  <span class="text-xs text-text-faint">Available tags: </span>
  {#each placeholders as ph (ph.key)}
    <button
      class="text-xs px-1.5 py-0.5 border border-border rounded-md bg-hover text-text-secondary font-inherit cursor-pointer transition-all duration-fast hover:bg-accent-bg hover:border-accent-muted hover:text-accent-text"
      class:!opacity-35={templateInput.includes('{' + ph.key + '}')}
      class:!cursor-default={templateInput.includes('{' + ph.key + '}')}
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

<details class="mt-2 mb-2">
  <summary class="text-xs text-text-faint cursor-pointer mb-1.5">Manual edit</summary>
  <input
    class="w-full box-border flex-1 px-2 py-1 border border-border rounded-lg bg-bg-input text-text text-sm font-inherit outline-none focus:border-focus-ring"
    bind:value={templateInput}
    oninput={() => onSave()}
  />
  <p class="text-sm text-text-muted m-0 mt-1.5 mb-3">
    Conditional: <code>{'{?parentKey}...{/parentKey}'}</code> — only renders if value exists.
  </p>
</details>

{#if sepPopup.visible}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-[1100]" onclick={closeSepPopup}>
    <div
      class="fixed flex items-center gap-1 px-2 py-1 bg-bg-overlay border border-border rounded-lg shadow-[0_4px_16px_var(--color-scrim)] -translate-x-1/2 -translate-y-full -mt-2"
      style="left:{sepPopup.x}px;top:{sepPopup.y}px"
      onclick={(e) => e.stopPropagation()}
    >
      <span class="text-2xs text-text-muted mr-0.5">Separator:</span>
      {#each SEPARATORS as sep (sep)}
        <button
          class="px-2.5 py-[3px] border border-border rounded-md bg-hover text-text-secondary text-sm font-inherit cursor-pointer hover:bg-accent-bg hover:border-accent-muted hover:text-accent-text"
          onclick={() => confirmSeparatorAndAdd(sep)}
        >
          <code>{sep}</code>
        </button>
      {/each}
    </div>
  </div>
{/if}

{#if editSepPopup.visible}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-[1100]" onclick={closeEditSepPopup}>
    <div
      class="fixed flex items-center gap-1 px-2 py-1 bg-bg-overlay border border-border rounded-lg shadow-[0_4px_16px_var(--color-scrim)] -translate-x-1/2 -translate-y-full -mt-2"
      style="left:{editSepPopup.x}px;top:{editSepPopup.y}px"
      onclick={(e) => e.stopPropagation()}
    >
      <span class="text-2xs text-text-muted mr-0.5">Change to:</span>
      {#each SEPARATORS as sep (sep)}
        <button
          class="px-2.5 py-[3px] border border-border rounded-md bg-hover text-text-secondary text-sm font-inherit cursor-pointer hover:bg-accent-bg hover:border-accent-muted hover:text-accent-text"
          onclick={() => changeSeparator(sep)}
        >
          <code>{sep}</code>
        </button>
      {/each}
      <button
        class="px-2.5 py-[3px] border border-border rounded-md bg-hover text-danger-text text-sm font-inherit cursor-pointer hover:bg-danger-bg hover:border-danger-text"
        onclick={removeSeparatorToken}>x</button
      >
    </div>
  </div>
{/if}
