<script lang="ts">
  import { onDestroy } from 'svelte'
  import MessageStream from './MessageStream.svelte'
  import ChatInput from '../molecules/ChatInput.svelte'
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

  interface Props {
    conversationId: string
    /** Called when the user triggers "new chat" from within the pane. */
    onNewChat?: () => void
  }

  let { conversationId }: Props = $props()

  let inputValue = $state('')
  let lastRetryable = $state<string | null>(null)

  let state = $derived(sdkSessions[conversationId])
  let isStreaming = $derived(state?.status === 'streaming')
  let lastError = $derived(state?.lastError ?? null)

  $effect(() => {
    const id = conversationId
    void openConversation(id)
    return () => closeConversation(id)
  })

  function handleSubmit(text: string): void {
    if (!text.trim()) return
    lastRetryable = text
    void sendMessage(conversationId, text)
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

  onDestroy(() => {
    closeConversation(conversationId)
  })
</script>

<svelte:window onkeydown={onKeydown} />

<section class="sdk-chat-pane" aria-label="Agent chat">
  <MessageStream
    messages={state?.messages ?? []}
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
    <ChatInput
      bind:value={inputValue}
      placeholder="Message the agent… (⌘↵ to send)"
      disabled={isStreaming}
      onsubmit={handleSubmit}
    />
    {#if isStreaming}
      <button type="button" class="cancel-btn" onclick={handleCancel} title="Stop generation (⌘.)">
        Stop
      </button>
    {/if}
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

  .input-row {
    display: flex;
    align-items: flex-end;
    gap: 10px;
    padding: 10px 14px 14px;
    border-top: 1px solid var(--c-border-subtle);
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
