<script lang="ts">
  import { untrack } from 'svelte'
  import MessageStream from './MessageStream.svelte'
  import ChatInput from '../molecules/ChatInput.svelte'
  import AttachmentTray from '../molecules/AttachmentTray.svelte'
  import AttachmentChip from '../molecules/AttachmentChip.svelte'
  import AssistantErrorBlock from '../molecules/AssistantErrorBlock.svelte'
  import {
    cancel,
    closeConversation,
    openConversation,
    respondPermission,
    respondPlan,
    respondQuestion,
    sdkSessions,
    sendMessage,
  } from '../../../lib/stores/sdkAgentSessions.svelte'
  import { parseSlashCommand } from '../../../lib/chat/slashCommands'
  import type {
    Attachment as SdkAttachment,
    PermissionMode as SdkPermissionMode,
  } from '../../../../../main/sdkAgents/types'

  interface Props {
    conversationId: string
    /** Called when the user triggers "new chat" from within the pane. */
    onNewChat?: () => void
  }

  type PendingAttachment = SdkAttachment & { previewDataUrl?: string }

  let { conversationId, onNewChat }: Props = $props()

  let inputValue = $state('')
  let lastRetryable = $state<string | null>(null)
  let pendingModelOverride = $state<string | null>(null)
  let pendingModeOverride = $state<SdkPermissionMode | null>(null)
  let transientNotice = $state<string | null>(null)
  let clearedBefore = $state<string | null>(null)
  let pendingAttachments = $state<PendingAttachment[]>([])

  let state = $derived(sdkSessions[conversationId])
  let isStreaming = $derived(state?.status === 'streaming')
  let lastError = $derived(state?.lastError ?? null)
  let visibleMessages = $derived.by(() => {
    const all = state?.messages ?? []
    if (!clearedBefore) return all
    return all.filter((m) => m.createdAt > clearedBefore!)
  })

  $effect(() => {
    const id = conversationId
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
    const parsed = parseSlashCommand(text)
    switch (parsed.kind) {
      case 'new':
        onNewChat?.()
        return
      case 'clear':
        clearedBefore = new Date().toISOString()
        flashNotice('Transcript hidden (DB untouched)')
        return
      case 'retry':
        if (lastRetryable) void sendMessage(conversationId, lastRetryable)
        else flashNotice('Nothing to retry yet.')
        return
      case 'model':
        pendingModelOverride = parsed.model
        flashNotice(`Model override: ${parsed.model} (applies to next message)`)
        return
      case 'mode':
        pendingModeOverride = parsed.mode
        flashNotice(`Permission mode: ${parsed.mode} (applies to next message)`)
        return
      case 'invalid':
        flashNotice(parsed.reason)
        return
    }
    const body = parsed.text
    lastRetryable = body
    const modelOverride = pendingModelOverride ?? undefined
    const permissionModeOverride = pendingModeOverride ?? undefined
    const attachments = pendingAttachments.length > 0 ? pendingAttachments : undefined
    pendingModelOverride = null
    pendingModeOverride = null
    pendingAttachments = []
    void sendMessage(conversationId, body, {
      modelOverride,
      permissionModeOverride,
      attachments,
    })
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
        conversationId,
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
    void sendMessage(conversationId, lastRetryable)
  }

  function handleCancel(): void {
    void cancel(conversationId)
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

<section
  class="sdk-chat-pane"
  aria-label="Agent chat"
  onpaste={handlePaste}
  ondrop={handleDrop}
  ondragover={handleDragOver}
>
  {#if transientNotice}
    <div class="transient-notice" role="status">{transientNotice}</div>
  {/if}
  <MessageStream
    messages={visibleMessages}
    conversationModel={state?.conversation?.model ?? null}
    toolEvents={state?.toolEvents ?? {}}
    pendingAttention={state?.pendingAttention ?? []}
    {isStreaming}
    onRespondPermission={(requestId, decision) =>
      void respondPermission(conversationId, requestId, decision)}
    onRespondQuestion={(requestId, answers) =>
      void respondQuestion(conversationId, requestId, answers)}
    onRespondPlan={(requestId, decision) => void respondPlan(conversationId, requestId, decision)}
  />

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
      <ChatInput
        bind:value={inputValue}
        placeholder="Message the agent… (⌘↵ to send)"
        disabled={isStreaming}
        onsubmit={handleSubmit}
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
      </ChatInput>
      {#if isStreaming}
        <button
          type="button"
          class="cancel-btn"
          onclick={handleCancel}
          title="Stop generation (⌘.)"
        >
          Stop
        </button>
      {/if}
    </div>
  </div>
</section>

<style>
  .sdk-chat-pane {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--c-bg);
  }

  .error-row {
    padding: 6px 14px 0;
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
    justify-content: center;
    padding: 10px 14px 14px;
    border-top: 1px solid var(--c-border-subtle);
  }

  .input-column {
    width: 100%;
    max-width: 600px;
    display: flex;
    align-items: flex-end;
    gap: 10px;
  }

  .cancel-btn {
    flex-shrink: 0;
    padding: 8px 14px;
    background: var(--c-danger-bg);
    color: var(--c-danger-text);
    border: 1px solid color-mix(in srgb, var(--c-danger) 40%, transparent);
    border-radius: 6px;
    font-size: 12.5px;
    font-family: inherit;
    cursor: pointer;
  }

  .cancel-btn:hover {
    background: var(--c-danger);
    color: var(--c-bg);
    border-color: var(--c-danger);
  }
</style>
