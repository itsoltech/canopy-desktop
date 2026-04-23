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
  import EffortPickerInline from '../molecules/EffortPickerInline.svelte'
  import SlashCommandHint from '../molecules/SlashCommandHint.svelte'
  import FilePathHint from '../molecules/FilePathHint.svelte'
  import ShortcutsOverlay from './ShortcutsOverlay.svelte'
  import HistorySearchOverlay from './HistorySearchOverlay.svelte'
  import type { CustomSelectApi } from '../../shared/CustomSelect.svelte'
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
    setEffortLevel,
    setContextWindow,
  } from '../../../lib/stores/sdkAgentSessions.svelte'
  import { parseSlashCommand, SLASH_COMMAND_HINTS } from '../../../lib/chat/slashCommands'
  import {
    MODE_OPTIONS,
    cyclePermissionMode,
    makeHistoryNavigator,
    type HistoryNavigator,
  } from '../../../lib/chat/chatKeybindings'
  import { prefs } from '../../../lib/stores/preferences.svelte'
  import {
    normalizeClaudeProviderPreset,
    type ClaudeProviderPresetId,
  } from '../../../../../shared/claudeProviderPresets'
  import type {
    Attachment as SdkAttachment,
    EffortLevel as SdkEffortLevel,
    PermissionMode as SdkPermissionMode,
  } from '../../../../../main/sdkAgents/types'

  const DEFAULT_TERMINAL_FONT_FAMILY =
    'JetBrains Mono, JetBrainsMono Nerd Font, JetBrainsMono NF, FiraCode Nerd Font, Fira Code, Menlo, monospace'
  const DEFAULT_TERMINAL_FONT_SIZE = 13

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
    reasoning?: boolean
    contextWindow?: number | null
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
  let forceExpand = $state(false)
  let shortcutsOverlay = $state(false)
  let historySearchOverlay = $state(false)
  let composerTextarea = $state<HTMLTextAreaElement | undefined>(undefined)
  let modelPickerApi = $state<CustomSelectApi | null>(null)
  let effortPickerApi = $state<CustomSelectApi | null>(null)
  let historyNavigator: HistoryNavigator | null = null

  let state = $derived(sdkSessions[currentConversationId])
  let isStreaming = $derived(state?.status === 'streaming')
  let lastError = $derived(state?.lastError ?? null)
  let currentModel = $derived(state?.conversation?.model ?? 'sonnet')
  let currentMode: SdkPermissionMode = $derived(state?.conversation?.permissionMode ?? 'default')
  let currentEffort: SdkEffortLevel | null = $derived(state?.conversation?.effortLevel ?? null)
  // Reasoning gating: only show the effort picker when the resolved model
  // advertises reasoning in the catalog. Aliases ("sonnet") resolve through
  // the same matchesFamily lookup the groups use so inline picks stay honest.
  let resolvedModelOption = $derived.by<SelectOption | null>(() => {
    const direct = providerModelOptions.find((o) => o.value === currentModel)
    if (direct) return direct
    if (activeProviderPreset === 'anthropic') {
      const alias =
        currentModel === 'haiku' || currentModel === 'sonnet' || currentModel === 'opus'
          ? currentModel
          : null
      if (alias) {
        const resolved = providerModelOptions.find((o) => matchesFamily(o, alias))
        if (resolved) return resolved
      }
    }
    return null
  })
  let currentModelReasoning = $derived(resolvedModelOption?.reasoning === true)

  // Push the active model's context window into the session so the statusbar
  // and extras panel can render `ctx NN%` without round-tripping to main.
  $effect(() => {
    const id = currentConversationId
    const window = resolvedModelOption?.contextWindow ?? null
    setContextWindow(id, window)
  })
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

  // `@`-mention file autocomplete. Tracks the `@` position so we can compute
  // the query and replace it on accept without a second parse pass.
  let fileMentionStart = $state<number | null>(null)
  let fileMentionQuery = $state('')
  let fileMentionDismissed = $state(false)
  let fileMatches = $state<string[]>([])
  let fileFocus = $state(0)

  // Detect an in-progress `@token` at the end of the input (preceded by BOL or
  // whitespace). Any further whitespace breaks the mention and closes the menu.
  $effect(() => {
    if (fileMentionDismissed) {
      fileMentionStart = null
      fileMentionQuery = ''
      return
    }
    const v = inputValue
    const m = v.match(/(?:^|\s)@([^\s@]*)$/)
    if (m) {
      fileMentionStart = v.length - (m[1]?.length ?? 0) - 1
      fileMentionQuery = m[1] ?? ''
    } else {
      fileMentionStart = null
      fileMentionQuery = ''
    }
  })

  $effect(() => {
    const query = fileMentionQuery
    const workspacePath = state?.conversation?.worktreePath
    if (fileMentionStart === null || !workspacePath) {
      fileMatches = []
      return
    }
    let cancelled = false
    window.api
      .listWorkspaceFiles({ workspacePath, query, limit: 20 })
      .then((files) => {
        if (cancelled) return
        fileMatches = files
        if (fileFocus >= files.length) fileFocus = 0
      })
      .catch(() => {
        if (!cancelled) fileMatches = []
      })
    return () => {
      cancelled = true
    }
  })

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

  // User-submitted prompts for Up-arrow recall and Cmd+R search. Newest first.
  let userPrompts = $derived.by<string[]>(() => {
    const all = state?.messages ?? []
    const out: string[] = []
    for (let i = all.length - 1; i >= 0; i--) {
      const m = all[i]
      if (m && m.role === 'user' && m.content.trim().length > 0) out.push(m.content)
    }
    return out
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
      fileMentionDismissed = false
      fileMentionStart = null
      fileMatches = []
      fileFocus = 0
      historyNavigator = null
      forceExpand = false
      shortcutsOverlay = false
      historySearchOverlay = false
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
    historyNavigator = null
    void sendMessage(currentConversationId, body, { attachments })
  }

  function acceptFileHint(path: string): void {
    if (fileMentionStart === null) return
    const before = inputValue.slice(0, fileMentionStart)
    const after = inputValue.slice(fileMentionStart + 1 + fileMentionQuery.length)
    const suffix = after.length === 0 || after.startsWith(' ') ? '' : ' '
    inputValue = `${before}@${path}${suffix}${after}`
    fileMentionDismissed = true
    queueMicrotask(() => {
      if (!composerTextarea) return
      const caret = before.length + 1 + path.length + (suffix ? 1 : 0)
      composerTextarea.focus()
      composerTextarea.setSelectionRange(caret, caret)
    })
  }

  function navigateHistory(direction: 'up' | 'down'): boolean {
    if (userPrompts.length === 0) return false
    if (!historyNavigator) historyNavigator = makeHistoryNavigator(userPrompts)
    const next = direction === 'up' ? historyNavigator.up(inputValue) : historyNavigator.down()
    if (next === null) return false
    inputValue = next
    slashDismissed = true
    fileMentionDismissed = true
    queueMicrotask(() => {
      if (!composerTextarea) return
      const len = composerTextarea.value.length
      composerTextarea.setSelectionRange(len, len)
    })
    return true
  }

  function caretOnFirstLine(): boolean {
    if (!composerTextarea) return true
    const caret = composerTextarea.selectionStart ?? 0
    return !composerTextarea.value.slice(0, caret).includes('\n')
  }

  function caretOnLastLine(): boolean {
    if (!composerTextarea) return true
    const caret = composerTextarea.selectionEnd ?? 0
    return !composerTextarea.value.slice(caret).includes('\n')
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
    // Slash-command menu has priority when open.
    if (slashMatches.length > 0) {
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
    }

    // File-mention menu next.
    if (fileMentionStart !== null && fileMatches.length > 0) {
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        fileFocus = (fileFocus + 1) % fileMatches.length
        return true
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault()
        fileFocus = (fileFocus - 1 + fileMatches.length) % fileMatches.length
        return true
      }
      if (e.key === 'Enter' || e.key === 'Tab') {
        e.preventDefault()
        const pick = fileMatches[fileFocus]
        if (pick) acceptFileHint(pick)
        return true
      }
      if (e.key === 'Escape') {
        e.preventDefault()
        fileMentionDismissed = true
        return true
      }
    }

    // Up/Down history navigation when composer is on its edge.
    if (e.key === 'ArrowUp' && !e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey) {
      if (inputValue.length === 0 || caretOnFirstLine()) {
        if (navigateHistory('up')) {
          e.preventDefault()
          return true
        }
      }
    }
    if (e.key === 'ArrowDown' && !e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey) {
      if (historyNavigator?.active && caretOnLastLine()) {
        if (navigateHistory('down')) {
          e.preventDefault()
          return true
        }
      }
    }

    return false
  }

  function handleInputChange(): void {
    // Any input change un-dismisses the menus so they can re-appear on retype.
    slashDismissed = false
    fileMentionDismissed = false
    // Leaving history mode when the user edits the recalled prompt.
    if (historyNavigator?.active) {
      historyNavigator = null
    }
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

  function isEventInPane(e: KeyboardEvent): boolean {
    const target = e.target
    if (!paneEl) return false
    if (target instanceof Node) return paneEl.contains(target)
    return false
  }

  function onKeydown(e: KeyboardEvent): void {
    // Don't intercept keystrokes that belong to a different pane / modal.
    if (!isEventInPane(e)) return

    // Overlays handle their own keys (Esc, ?). Skip global handling entirely.
    if (shortcutsOverlay || historySearchOverlay) return

    // ⌘. / Ctrl+. cancels the in-flight request.
    if (e.key === '.' && (e.metaKey || e.ctrlKey) && isStreaming) {
      e.preventDefault()
      handleCancel()
      return
    }

    // Esc cancels streaming when no pop-up owns it.
    if (e.key === 'Escape' && isStreaming) {
      // Slash / file menus consume Esc first (via onKeydownIntercept inside textarea).
      if (slashMatches.length > 0 && !slashDismissed) return
      if (fileMentionStart !== null && !fileMentionDismissed && fileMatches.length > 0) return
      e.preventDefault()
      handleCancel()
      return
    }

    // Shift+Tab cycles the permission mode.
    if (e.key === 'Tab' && e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault()
      const next = cyclePermissionMode(currentMode)
      void setPermissionMode(currentConversationId, next)
      flashNotice(`Permission mode: ${MODE_OPTIONS.find((m) => m.value === next)?.label ?? next}`)
      return
    }

    // Cmd/Ctrl+R opens the prompt history search.
    if (e.key === 'r' && (e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey) {
      e.preventDefault()
      historySearchOverlay = true
      return
    }

    // Ctrl+O toggles expand-all for tool / thinking / subagent blocks.
    if (e.key === 'o' && e.ctrlKey && !e.metaKey && !e.shiftKey && !e.altKey) {
      e.preventDefault()
      forceExpand = !forceExpand
      flashNotice(forceExpand ? 'Expanded all blocks' : 'Collapsed all blocks')
      return
    }

    // Alt+P / Option+P opens the model picker. `e.code` guards against
    // macOS Option producing a diacritic as `e.key`.
    if (e.altKey && !e.metaKey && !e.ctrlKey && !e.shiftKey && e.code === 'KeyP') {
      e.preventDefault()
      modelPickerApi?.open()
      return
    }

    // Alt+T / Option+T opens the effort picker for reasoning models.
    if (e.altKey && !e.metaKey && !e.ctrlKey && !e.shiftKey && e.code === 'KeyT') {
      if (!currentModelReasoning) return
      e.preventDefault()
      effortPickerApi?.open()
      return
    }

    // `?` opens the shortcuts cheat sheet when no text is being typed.
    if (
      e.key === '?' &&
      !e.metaKey &&
      !e.ctrlKey &&
      !e.altKey &&
      inputValue.length === 0 &&
      slashMatches.length === 0 &&
      fileMentionStart === null
    ) {
      e.preventDefault()
      shortcutsOverlay = true
    }
  }

  function handleHistoryAccept(prompt: string, andSend: boolean): void {
    historySearchOverlay = false
    inputValue = prompt
    queueMicrotask(() => {
      composerTextarea?.focus()
      const len = composerTextarea?.value.length ?? 0
      composerTextarea?.setSelectionRange(len, len)
      if (andSend) handleSubmit(prompt)
    })
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
      {forceExpand}
      composerKey={`${inputValue.length}:${pendingAttachments.length}:${isStreaming ? 'streaming' : 'idle'}:${slashMatches.length}:${fileMatches.length}`}
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
            {:else if fileMentionStart !== null && fileMatches.length > 0}
              <div class="slash-hints-panel" role="listbox" aria-label="File paths">
                <div class="slash-hints">
                  {#each fileMatches as match, i (match)}
                    <FilePathHint
                      path={match}
                      focused={i === fileFocus}
                      onselect={() => acceptFileHint(match)}
                    />
                  {/each}
                </div>
              </div>
            {/if}
            <ChatInput
              bind:value={inputValue}
              placeholder="Message the agent… (⌘↵ to send, ? for shortcuts)"
              disabled={isStreaming}
              onsubmit={handleSubmit}
              onchange={handleInputChange}
              onstop={handleCancel}
              onKeydownIntercept={handleKeydownIntercept}
              onTextareaReady={(el) => (composerTextarea = el)}
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
                  onready={(api) => (modelPickerApi = api)}
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
              {#snippet effortPicker()}
                {#if currentModelReasoning}
                  <EffortPickerInline
                    value={currentEffort}
                    onchange={(v) =>
                      void setEffortLevel(currentConversationId, v as SdkEffortLevel | null)}
                    onready={(api) => (effortPickerApi = api)}
                  />
                {/if}
              {/snippet}
            </ChatInput>
          </div>
        </div>
      {/snippet}
    </MessageStream>
    {#if shortcutsOverlay}
      <ShortcutsOverlay onclose={() => (shortcutsOverlay = false)} />
    {/if}
    {#if historySearchOverlay}
      <HistorySearchOverlay
        prompts={userPrompts}
        onaccept={handleHistoryAccept}
        onclose={() => (historySearchOverlay = false)}
      />
    {/if}
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
