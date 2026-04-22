<script lang="ts">
  import { untrack } from 'svelte'
  import { History, X } from '@lucide/svelte'
  import MessageStream from './MessageStream.svelte'
  import ConversationListSidebar from './ConversationListSidebar.svelte'
  import ChatInput from '../molecules/ChatInput.svelte'
  import AttachmentTray from '../molecules/AttachmentTray.svelte'
  import AttachmentChip from '../molecules/AttachmentChip.svelte'
  import AssistantErrorBlock from '../molecules/AssistantErrorBlock.svelte'
  import ModelPickerInline from '../molecules/ModelPickerInline.svelte'
  import PermissionModePicker from '../molecules/PermissionModePicker.svelte'
  import SlashCommandHint from '../molecules/SlashCommandHint.svelte'
  import {
    cancel,
    closeConversation,
    openConversation,
    respondPermission,
    respondPlan,
    respondQuestion,
    sdkSessions,
    sendMessage,
    setModel,
    setPermissionMode,
  } from '../../../lib/stores/sdkAgentSessions.svelte'
  import { parseSlashCommand, SLASH_COMMAND_HINTS } from '../../../lib/chat/slashCommands'
  import { prefs } from '../../../lib/stores/preferences.svelte'
  import {
    normalizeClaudeProviderPreset,
    type ClaudeProviderPresetId,
  } from '../../../../../shared/claudeProviderPresets'
  import type {
    Attachment as SdkAttachment,
    PermissionMode as SdkPermissionMode,
  } from '../../../../../main/sdkAgents/types'

  const DEFAULT_TERMINAL_FONT_FAMILY =
    'JetBrains Mono, JetBrainsMono Nerd Font, JetBrainsMono NF, FiraCode Nerd Font, Fira Code, Menlo, monospace'
  const DEFAULT_TERMINAL_FONT_SIZE = 13

  const MODE_OPTIONS: { value: SdkPermissionMode; label: string }[] = [
    { value: 'default', label: 'Default' },
    { value: 'plan', label: 'Plan' },
    { value: 'acceptEdits', label: 'Auto-accept edits' },
    { value: 'bypassPermissions', label: 'Bypass permissions' },
  ]

  interface Props {
    conversationId: string
    /** Called when the user triggers "new chat" from within the pane. */
    onNewChat?: () => void
  }

  type PendingAttachment = SdkAttachment & { previewDataUrl?: string }
  interface SelectOption {
    value: string
    label: string
    family?: string | null
    releaseDate?: string | null
    lastUpdated?: string | null
  }
  interface SelectGroup {
    label: string
    options: SelectOption[]
  }
  interface SlashMatch {
    command: string
    description: string
    insertText: string
  }

  function matchesFamily(option: SelectOption, family: string): boolean {
    const normalized = family.toLowerCase()
    return (
      option.family === normalized ||
      option.family === `claude-${normalized}` ||
      option.label.toLowerCase().includes(normalized)
    )
  }

  function inferPresetFromModel(model: string): ClaudeProviderPresetId {
    const value = model.toLowerCase()
    if (value.includes('minimax')) return 'minimax'
    if (value.includes('kimi')) return 'kimi'
    if (value.includes('glm') || value.includes('zai') || value.includes('zhipu')) return 'zai'
    return 'anthropic'
  }

  let { conversationId, onNewChat }: Props = $props()

  // Sidebar can swap the viewed conversation in place. `overrideId` holds
  // the user's choice; when null, we fall back to the outer prop. Clearing it
  // (or an outer prop change) snaps back to the pane's original conversation.
  let overrideId = $state<string | null>(null)
  let currentConversationId = $derived(overrideId ?? conversationId)

  let inputValue = $state('')
  let lastRetryable = $state<string | null>(null)
  let transientNotice = $state<string | null>(null)
  let clearedBefore = $state<string | null>(null)
  let clearedAttentionIds = $state<Set<string>>(new Set())
  let pendingAttachments = $state<PendingAttachment[]>([])
  let historyOpen = $state(false)
  let lastViewedConversationId = $state(currentConversationId)
  let paneEl: HTMLElement | undefined = $state()
  let activeProviderPreset = $state<ClaudeProviderPresetId>('anthropic')
  let providerModelOptions = $state<SelectOption[]>([])

  let state = $derived(sdkSessions[currentConversationId])
  let isStreaming = $derived(state?.status === 'streaming')
  let lastError = $derived(state?.lastError ?? null)
  let currentModel = $derived(state?.conversation?.model ?? 'sonnet')
  let currentMode: SdkPermissionMode = $derived(state?.conversation?.permissionMode ?? 'default')
  let terminalFontFamily = $derived(prefs.fontFamily || DEFAULT_TERMINAL_FONT_FAMILY)
  let terminalFontSize = $derived(
    Number.parseInt(prefs.fontSize || '', 10) || DEFAULT_TERMINAL_FONT_SIZE,
  )
  // If the conversation carries a model name that's not in the preset list,
  // surface it as a disabled-looking option so the dropdown still shows the label.
  let modelOptions = $derived.by(() => {
    if (providerModelOptions.some((o) => o.value === currentModel)) return providerModelOptions

    if (activeProviderPreset === 'anthropic') {
      const aliasFamily =
        currentModel === 'haiku' || currentModel === 'sonnet' || currentModel === 'opus'
          ? currentModel
          : null
      if (aliasFamily) {
        const resolved = providerModelOptions.find((option) => matchesFamily(option, aliasFamily))
        if (resolved) {
          return [{ ...resolved, value: currentModel }, ...providerModelOptions]
        }
      }
    }

    return [{ value: currentModel, label: currentModel }, ...providerModelOptions]
  })
  let modelGroups = $derived.by<SelectGroup[] | undefined>(() => {
    if (modelOptions.length === 0) return undefined

    // Only surface a "Custom" group when the current model isn't in the
    // provider's catalog — otherwise the catalog entry is highlighted in
    // place and we don't duplicate it under a synthetic "Selected" header.
    const hasCustomSelection =
      currentModel !== '' && !providerModelOptions.some((option) => option.value === currentModel)
    const custom = hasCustomSelection
      ? modelOptions.find((option) => option.value === currentModel)
      : undefined
    const catalog = hasCustomSelection
      ? modelOptions.filter((option) => option.value !== currentModel)
      : modelOptions
    const groups: SelectGroup[] = []

    if (custom) groups.push({ label: 'Custom', options: [custom] })

    if (activeProviderPreset === 'anthropic') {
      for (const family of ['Haiku', 'Opus', 'Sonnet']) {
        const options = catalog.filter((option) => matchesFamily(option, family))
        if (options.length > 0) groups.push({ label: family, options })
      }

      const other = catalog.filter(
        (option) =>
          !matchesFamily(option, 'Haiku') &&
          !matchesFamily(option, 'Opus') &&
          !matchesFamily(option, 'Sonnet'),
      )
      if (other.length > 0) groups.push({ label: 'Other', options: other })
      return groups
    }

    if (catalog.length > 0) groups.push({ label: 'Models', options: catalog })
    return groups
  })
  let visibleMessages = $derived.by(() => {
    const all = state?.messages ?? []
    if (!clearedBefore) return all
    return all.filter((m) => m.createdAt > clearedBefore!)
  })
  let visibleMessageIds = $derived(new Set(visibleMessages.map((message) => message.id)))
  let visiblePendingAttention = $derived.by(() => {
    const blocks = state?.pendingAttention ?? []
    return blocks.filter((block) => {
      if (clearedAttentionIds.has(block.requestId)) return false
      if (!clearedBefore) return true
      return block.messageId ? visibleMessageIds.has(block.messageId) : true
    })
  })

  // Slash-command autocomplete, including argument suggestions for commands
  // with known parameter sets like /model and /mode.
  let slashFocus = $state(0)
  let slashDismissed = $state(false)

  let slashMatches = $derived.by<SlashMatch[]>(() => {
    if (slashDismissed) return []
    const v = inputValue
    if (!v.startsWith('/')) return []
    if (/\n/.test(v)) return []

    const raw = v.slice(1)
    const hasTrailingSpace = /\s$/.test(v)
    const tokens = raw.split(/\s+/).filter(Boolean)

    if (tokens.length <= 1 && !hasTrailingSpace) {
      const q = raw.toLowerCase()
      return SLASH_COMMAND_HINTS.filter((hint) =>
        hint.command.slice(1).toLowerCase().startsWith(q),
      ).map((hint) => ({
        command: hint.command,
        description: hint.description,
        insertText: `${hint.command.split(' ')[0]} `,
      }))
    }

    const head = tokens[0]?.toLowerCase()
    const argQuery = (hasTrailingSpace ? '' : tokens.slice(1).join(' ')).toLowerCase()

    if (head === 'model') {
      return modelOptions
        .filter(
          (option) =>
            argQuery.length === 0 ||
            option.value.toLowerCase().includes(argQuery) ||
            option.label.toLowerCase().includes(argQuery),
        )
        .map((option) => ({
          command: `/model ${option.value}`,
          description: `Use ${option.label} for the next message.`,
          insertText: `/model ${option.value}`,
        }))
    }

    if (head === 'mode') {
      return MODE_OPTIONS.filter(
        (option) =>
          argQuery.length === 0 ||
          option.value.toLowerCase().includes(argQuery) ||
          option.label.toLowerCase().includes(argQuery),
      ).map((option) => ({
        command: `/mode ${option.value}`,
        description: `${option.label} for the next message.`,
        insertText: `/mode ${option.value}`,
      }))
    }

    return []
  })

  // Keep focus index in range as matches change.
  $effect(() => {
    void slashMatches.length
    if (slashFocus >= slashMatches.length) slashFocus = 0
  })

  $effect(() => {
    if (currentConversationId !== lastViewedConversationId) {
      lastViewedConversationId = currentConversationId
      clearedBefore = null
      clearedAttentionIds = new Set()
      inputValue = ''
      pendingAttachments = []
      slashFocus = 0
      slashDismissed = false
    }
  })

  $effect(() => {
    const profileId = state?.conversation?.agentProfileId
    if (!profileId) {
      activeProviderPreset = inferPresetFromModel(currentModel)
      providerModelOptions = []
      return
    }

    let cancelled = false
    window.api.getProfile(profileId).then((profile) => {
      if (cancelled) return
      activeProviderPreset = normalizeClaudeProviderPreset(profile?.prefs.claudeProviderPreset)
    })
    return () => {
      cancelled = true
    }
  })

  $effect(() => {
    const preset = activeProviderPreset
    let cancelled = false
    providerModelOptions = []
    window.api
      .getClaudeProviderModels(preset)
      .then((options) => {
        if (cancelled) return
        if (options.length === 0) {
          const inferredPreset = inferPresetFromModel(currentModel)
          if (inferredPreset !== preset) activeProviderPreset = inferredPreset
          return
        }
        providerModelOptions = options
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  })

  $effect(() => {
    const id = currentConversationId
    untrack(() => {
      void openConversation(id)
    })
    return () => {
      untrack(() => closeConversation(id))
    }
  })

  function flashNotice(msg: string): void {
    transientNotice = msg
    setTimeout(() => {
      if (transientNotice === msg) transientNotice = null
    }, 3000)
  }

  function handleSubmit(text: string): void {
    if (!text.trim()) return
    slashDismissed = false
    const parsed = parseSlashCommand(text)
    switch (parsed.kind) {
      case 'new':
        onNewChat?.()
        return
      case 'clear':
        clearedBefore = new Date().toISOString()
        clearedAttentionIds = new Set(
          (state?.pendingAttention ?? []).map((block) => block.requestId),
        )
        flashNotice('Transcript hidden (DB untouched)')
        return
      case 'retry':
        if (lastRetryable) void sendMessage(currentConversationId, lastRetryable)
        else flashNotice('Nothing to retry yet.')
        return
      case 'model':
        void setModel(currentConversationId, parsed.model)
        flashNotice(`Model set to ${parsed.model}`)
        return
      case 'mode':
        void setPermissionMode(currentConversationId, parsed.mode)
        flashNotice(`Permission mode: ${parsed.mode}`)
        return
      case 'invalid':
        flashNotice(parsed.reason)
        return
    }
    const body = parsed.text
    lastRetryable = body
    const attachments = pendingAttachments.length > 0 ? pendingAttachments : undefined
    pendingAttachments = []
    void sendMessage(currentConversationId, body, { attachments })
  }

  function acceptHint(hint: SlashMatch): void {
    inputValue = hint.insertText
    slashFocus = 0
    slashDismissed = false
    // The Enter/Tab that triggered this came from the textarea, so it's still
    // the active element — move caret to end after Svelte flushes the value.
    queueMicrotask(() => {
      const el = document.activeElement
      if (el instanceof HTMLTextAreaElement) {
        el.setSelectionRange(inputValue.length, inputValue.length)
      }
    })
  }

  function handleKeydownIntercept(e: KeyboardEvent): boolean {
    if (slashMatches.length === 0) return false
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      slashFocus = (slashFocus + 1) % slashMatches.length
      return true
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault()
      slashFocus = (slashFocus - 1 + slashMatches.length) % slashMatches.length
      return true
    }
    if (e.key === 'Enter' || e.key === 'Tab') {
      e.preventDefault()
      const hint = slashMatches[slashFocus]
      if (hint) acceptHint(hint)
      return true
    }
    if (e.key === 'Escape') {
      e.preventDefault()
      slashDismissed = true
      return true
    }
    return false
  }

  function handleInputChange(): void {
    // Any input change un-dismisses the menu so it can re-appear on retype.
    slashDismissed = false
  }

  function removeAttachment(id: string): void {
    pendingAttachments = pendingAttachments.filter((a) => a.id !== id)
  }

  function attachmentChipKind(kind: SdkAttachment['kind']): 'image' | 'text' | 'file' {
    return kind === 'image' ? 'image' : 'text'
  }

  async function handlePaste(e: ClipboardEvent): Promise<void> {
    const items = e.clipboardData?.items
    if (!items) return
    for (const item of items) {
      if (item.kind !== 'file' || !item.type.startsWith('image/')) continue
      const file = item.getAsFile()
      if (!file) continue
      e.preventDefault()
      await uploadAttachmentFile(file, 'image')
    }
  }

  async function handleDrop(e: DragEvent): Promise<void> {
    e.preventDefault()
    const files = e.dataTransfer?.files
    if (!files || files.length === 0) return
    for (const file of files) {
      const kind: 'image' | 'text' = file.type.startsWith('image/') ? 'image' : 'text'
      await uploadAttachmentFile(file, kind)
    }
  }

  function handleDragOver(e: DragEvent): void {
    e.preventDefault()
  }

  async function uploadAttachmentFile(file: File, kind: 'image' | 'text'): Promise<void> {
    try {
      const buffer = await file.arrayBuffer()
      const base64 = bufferToBase64(buffer)
      const mimeType = file.type || (kind === 'image' ? 'image/png' : 'text/plain')
      const result = await window.api.sdkAgent.uploadAttachment({
        conversationId: currentConversationId,
        filename: file.name,
        mimeType,
        kind,
        dataBase64: base64,
      })
      if ('error' in result) {
        flashNotice(`Attachment failed: ${result.error._tag}`)
        return
      }
      pendingAttachments = [
        ...pendingAttachments,
        {
          ...(result as SdkAttachment),
          ...(kind === 'image' ? { previewDataUrl: `data:${mimeType};base64,${base64}` } : {}),
        },
      ]
    } catch (err) {
      flashNotice(`Attachment failed: ${err instanceof Error ? err.message : 'unknown'}`)
    }
  }

  function bufferToBase64(buffer: ArrayBuffer): string {
    const bytes = new Uint8Array(buffer)
    let binary = ''
    for (let i = 0; i < bytes.byteLength; i++) binary += String.fromCharCode(bytes[i])
    return btoa(binary)
  }

  function handleRetry(): void {
    if (!lastRetryable) return
    void sendMessage(currentConversationId, lastRetryable)
  }

  function appendToComposer(text: string): void {
    const prefix = inputValue.trim().length > 0 ? `${inputValue.trimEnd()}\n\n` : ''
    inputValue = `${prefix}${text}`
    queueMicrotask(() => {
      const textarea = paneEl?.querySelector('.chat-input .textarea')
      if (textarea instanceof HTMLTextAreaElement) {
        textarea.focus()
        textarea.setSelectionRange(textarea.value.length, textarea.value.length)
      }
    })
  }

  function formatQuoteForComposer(text: string, comment?: string): string {
    const quote = text
      .trim()
      .split('\n')
      .map((line) => `> ${line}`)
      .join('\n')
    if (!comment?.trim()) return quote
    return `${quote}\n\nComment: ${comment.trim()}`
  }

  function handleCancel(): void {
    void cancel(currentConversationId)
  }

  function onKeydown(e: KeyboardEvent): void {
    // ⌘. (macOS) or Ctrl+. (others) cancels the in-flight request.
    if (e.key === '.' && (e.metaKey || e.ctrlKey) && isStreaming) {
      e.preventDefault()
      handleCancel()
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="sdk-chat-layout">
  {#if historyOpen && state?.conversation}
    <ConversationListSidebar
      workspaceId={state.conversation.workspaceId}
      worktreePath={state.conversation.worktreePath}
      activeConversationId={currentConversationId}
      onOpen={(id) => {
        overrideId = id === conversationId ? null : id
        historyOpen = false
      }}
      onNew={() => {
        historyOpen = false
        onNewChat?.()
      }}
      onDeleted={(id) => {
        // If the user just purged the conversation we're viewing, snap back to
        // the pane's original prop so the UI doesn't keep staring at a dead id.
        if (id === currentConversationId) overrideId = null
      }}
    />
  {/if}
  <section
    bind:this={paneEl}
    class="sdk-chat-pane"
    aria-label="Agent chat"
    style:font-family={terminalFontFamily}
    style:font-size={`${terminalFontSize}px`}
    style:--font-mono={terminalFontFamily}
    onpaste={handlePaste}
    ondrop={handleDrop}
    ondragover={handleDragOver}
  >
    <header class="pane-toolbar">
      <button
        type="button"
        class="toolbar-btn"
        aria-pressed={historyOpen}
        title={historyOpen ? 'Hide history' : 'Show history'}
        onclick={() => (historyOpen = !historyOpen)}
      >
        {#if historyOpen}
          <X size={14} />
        {:else}
          <History size={14} />
        {/if}
        <span>History</span>
      </button>
    </header>
    {#if transientNotice}
      <div class="transient-notice" role="status">{transientNotice}</div>
    {/if}
    <MessageStream
      messages={visibleMessages}
      conversationModel={state?.conversation?.model ?? null}
      toolEvents={state?.toolEvents ?? {}}
      subagents={state?.subagents ?? {}}
      pendingAttention={visiblePendingAttention}
      {isStreaming}
      composerKey={`${inputValue.length}:${pendingAttachments.length}:${isStreaming ? 'streaming' : 'idle'}:${slashMatches.length}`}
      onRespondPermission={(requestId, decision) =>
        void respondPermission(currentConversationId, requestId, decision)}
      onRespondQuestion={(requestId, answers) =>
        void respondQuestion(currentConversationId, requestId, answers)}
      onRespondPlan={(requestId, decision) =>
        void respondPlan(currentConversationId, requestId, decision)}
      onCopySelection={(ok) => flashNotice(ok ? 'Copied to clipboard' : 'Failed to copy')}
      onQuoteSelection={(selection) =>
        appendToComposer(formatQuoteForComposer(selection.text, selection.comment))}
    >
      {#snippet composer()}
        {#if lastError}
          <div class="error-row">
            <AssistantErrorBlock
              errorTag={lastError}
              canRetry={!!lastRetryable}
              retrying={isStreaming}
              onretry={handleRetry}
            />
          </div>
        {/if}

        <div class="input-row">
          <div class="input-column">
            {#if slashMatches.length > 0}
              <div class="slash-hints-panel" role="listbox" aria-label="Slash commands">
                <div class="slash-hints">
                  {#each slashMatches as hint, i (hint.command)}
                    <SlashCommandHint
                      command={hint.command}
                      description={hint.description}
                      focused={i === slashFocus}
                      onselect={() => acceptHint(hint)}
                    />
                  {/each}
                </div>
              </div>
            {/if}
            <ChatInput
              bind:value={inputValue}
              placeholder="Message the agent… (⌘↵ to send)"
              disabled={isStreaming}
              onsubmit={handleSubmit}
              onchange={handleInputChange}
              onstop={handleCancel}
              onKeydownIntercept={handleKeydownIntercept}
            >
              {#snippet attachments()}
                {#if pendingAttachments.length > 0}
                  <AttachmentTray>
                    {#each pendingAttachments as att (att.id)}
                      <AttachmentChip
                        name={att.filename}
                        sizeBytes={att.sizeBytes}
                        kind={attachmentChipKind(att.kind)}
                        onremove={() => removeAttachment(att.id)}
                      />
                    {/each}
                  </AttachmentTray>
                {/if}
              {/snippet}
              {#snippet modelPicker()}
                <ModelPickerInline
                  value={currentModel}
                  options={modelOptions}
                  groups={modelGroups}
                  onchange={(v) => void setModel(currentConversationId, v)}
                />
              {/snippet}
              {#snippet permissionMode()}
                <PermissionModePicker
                  value={currentMode}
                  options={MODE_OPTIONS}
                  onchange={(v) =>
                    void setPermissionMode(currentConversationId, v as SdkPermissionMode)}
                />
              {/snippet}
            </ChatInput>
          </div>
        </div>
      {/snippet}
    </MessageStream>
  </section>
</div>

<style>
  .sdk-chat-layout {
    display: flex;
    height: 100%;
    min-height: 0;
    background: var(--c-bg);
  }

  .sdk-chat-pane {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    height: 100%;
    background: var(--c-bg);
    line-height: 1.45;
  }

  .pane-toolbar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    border-bottom: 1px solid var(--c-border-subtle);
    background: color-mix(in srgb, var(--c-bg) 92%, black);
  }

  .toolbar-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 7px;
    background: transparent;
    color: var(--c-text-muted);
    border: 1px solid var(--c-border-subtle);
    border-radius: 0;
    font-size: 0.88em;
    font-family: inherit;
    cursor: pointer;
  }

  .toolbar-btn:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .toolbar-btn[aria-pressed='true'] {
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
    border-color: color-mix(in srgb, var(--c-accent) 40%, transparent);
  }

  .error-row {
    padding: 0 0 6px;
  }

  .transient-notice {
    padding: 6px 14px;
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
    font-size: 11.5px;
    border-bottom: 1px solid color-mix(in srgb, var(--c-accent) 30%, transparent);
  }

  .input-row {
    display: flex;
    padding: 0;
  }

  .input-column {
    position: relative;
    width: 100%;
    display: flex;
    align-items: flex-end;
    gap: 10px;
  }

  .slash-hints-panel {
    position: absolute;
    left: 0;
    right: 0;
    bottom: calc(100% + 8px);
    z-index: 20;
    border: 1px solid var(--c-border);
    background: color-mix(in srgb, var(--c-bg) 94%, black);
    box-shadow: 0 10px 24px color-mix(in srgb, black 30%, transparent);
  }

  .slash-hints {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 4px;
  }
</style>
