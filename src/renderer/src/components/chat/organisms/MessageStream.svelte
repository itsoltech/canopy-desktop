<script lang="ts">
  import { match } from 'ts-pattern'
  import { ArrowDown } from '@lucide/svelte'
  import MessageBubble from '../molecules/MessageBubble.svelte'
  import MessageHeader from '../molecules/MessageHeader.svelte'
  import MessageMeta from '../molecules/MessageMeta.svelte'
  import ToolCallBlock from '../molecules/ToolCallBlock.svelte'
  import QuestionnaireBlock from '../molecules/QuestionnaireBlock.svelte'
  import PlanApprovalBlock from '../molecules/PlanApprovalBlock.svelte'
  import ToolPermissionBlock from '../molecules/ToolPermissionBlock.svelte'
  import type {
    AttentionBlock,
    SdkMessageView,
    SdkToolEventView,
  } from '../../../lib/stores/sdkAgentSessions.svelte'
  import type {
    AskUserQuestionAnswer as SdkAskUserQuestionAnswer,
    PlanDecision as SdkPlanDecision,
    ToolDecision as SdkToolDecision,
  } from '../../../../../main/sdkAgents/types'

  interface Props {
    messages: SdkMessageView[]
    conversationModel?: string | null
    toolEvents: Record<string, SdkToolEventView>
    pendingAttention: AttentionBlock[]
    isStreaming?: boolean
    onRespondPermission?: (requestId: string, decision: SdkToolDecision) => void
    onRespondQuestion?: (
      requestId: string,
      answers: Record<string, SdkAskUserQuestionAnswer>,
    ) => void
    onRespondPlan?: (requestId: string, decision: SdkPlanDecision) => void
  }

  let {
    messages,
    conversationModel = null,
    toolEvents,
    pendingAttention,
    isStreaming = false,
    onRespondPermission,
    onRespondQuestion,
    onRespondPlan,
  }: Props = $props()

  type Brand = 'ClaudeAI' | 'OpenAI' | 'Gemini'
  type BubbleRole = 'user' | 'assistant' | 'tool' | 'system'

  interface MessageGroup {
    id: string
    role: BubbleRole
    messages: SdkMessageView[]
  }

  function resolveBrand(model: string | null | undefined): Brand | undefined {
    if (!model) return undefined
    const m = model.toLowerCase()
    if (m.startsWith('claude')) return 'ClaudeAI'
    if (m.startsWith('gpt') || m.startsWith('o1') || m.startsWith('o3')) return 'OpenAI'
    if (m.startsWith('gemini')) return 'Gemini'
    return undefined
  }

  let containerEl: HTMLDivElement | undefined = $state()
  let autoScroll = $state(true)
  let scrollFrame: number | null = null

  // Track scroll position — pause auto-scroll when user scrolls away from bottom
  function onScroll(): void {
    if (!containerEl) return
    const { scrollTop, scrollHeight, clientHeight } = containerEl
    const distance = scrollHeight - scrollTop - clientHeight
    autoScroll = distance < 80
  }

  function scheduleScrollToBottom(): void {
    if (!containerEl || !autoScroll) return
    if (scrollFrame !== null) return
    scrollFrame = requestAnimationFrame(() => {
      scrollFrame = null
      if (!containerEl) return
      containerEl.scrollTop = containerEl.scrollHeight
    })
  }

  function scrollToLatest(): void {
    autoScroll = true
    if (!containerEl) return
    containerEl.scrollTop = containerEl.scrollHeight
  }

  $effect(() => {
    // Trigger on messages / attention changes; rAF-batched so a burst of
    // assistant.delta events coalesces into one scroll per animation frame.
    void messages.length
    void pendingAttention.length
    scheduleScrollToBottom()
  })

  // Also re-schedule on streaming-text growth (without mutating the message
  // array count). Reactive on the last message content length.
  let lastContentLength = $derived(messages[messages.length - 1]?.content.length ?? 0)
  $effect(() => {
    void lastContentLength
    scheduleScrollToBottom()
  })

  $effect(() => {
    return () => {
      if (scrollFrame !== null) cancelAnimationFrame(scrollFrame)
    }
  })

  function toolEventsForMessage(messageId: string): SdkToolEventView[] {
    return Object.values(toolEvents).filter((t) => t.messageId === messageId)
  }

  function toolStatus(ev: SdkToolEventView): 'idle' | 'running' | 'success' | 'error' {
    return match(ev.status)
      .with('running', () => 'running' as const)
      .with('error', () => 'error' as const)
      .with('done', () => 'success' as const)
      .exhaustive()
  }

  function resolvedAnswers(
    block: AttentionBlock,
  ): Record<string, SdkAskUserQuestionAnswer> | undefined {
    return match(block)
      .with(
        { kind: 'question', status: 'resolved' },
        () => ({}) as Record<string, SdkAskUserQuestionAnswer>,
      )
      .otherwise(() => undefined)
  }

  function bubbleRole(message: SdkMessageView): BubbleRole {
    return message.role === 'tool' ? 'tool' : message.role
  }

  function groupHasUsage(group: MessageGroup): boolean {
    return group.messages.some((message) => message.tokensIn !== null || message.tokensOut !== null)
  }

  function groupTokenTotal(group: MessageGroup): number {
    return group.messages.reduce(
      (sum, message) => sum + (message.tokensIn ?? 0) + (message.tokensOut ?? 0),
      0,
    )
  }

  let timelineAttention = $derived.by(() => {
    const byMessage: Record<string, AttentionBlock[]> = {}
    const orphan: AttentionBlock[] = []

    for (const block of pendingAttention) {
      const messageId = block.messageId
      const hasMessage = messageId ? messages.some((message) => message.id === messageId) : false
      if (!messageId || !hasMessage) {
        orphan.push(block)
        continue
      }
      byMessage[messageId] = [...(byMessage[messageId] ?? []), block]
    }

    return { byMessage, orphan }
  })

  let messageGroups = $derived.by<MessageGroup[]>(() => {
    const groups: MessageGroup[] = []

    for (const message of messages) {
      const role = bubbleRole(message)
      const previous = groups[groups.length - 1]
      if (role === 'assistant' && previous?.role === 'assistant') {
        previous.messages = [...previous.messages, message]
        continue
      }
      groups.push({
        id: message.id,
        role,
        messages: [message],
      })
    }

    return groups
  })
</script>

<div class="stream-wrapper">
  <div class="message-stream" bind:this={containerEl} onscroll={onScroll}>
    <div class="stream-column">
      {#if messages.length === 0 && pendingAttention.length === 0}
        <div class="empty-state">
          <p>Start a conversation. The assistant is ready to help with this worktree.</p>
        </div>
      {/if}

      {#each messageGroups as group (group.id)}
        {@const firstMessage = group.messages[0]}
        {@const isAssistant = group.role === 'assistant'}
        <MessageBubble role={group.role}>
          {#snippet header()}
            <MessageHeader
              role={group.role === 'user' ? 'user' : 'assistant'}
              brand={isAssistant ? resolveBrand(conversationModel) : undefined}
              model={isAssistant ? (conversationModel ?? undefined) : undefined}
              userInitial={group.role === 'user' ? 'Y' : undefined}
              timestamp={firstMessage.createdAt}
            />
          {/snippet}

          {#snippet body()}
            <div class="message-body">
              {#each group.messages as message (message.id)}
                <div class="message-segment">
                  {#if message.content}
                    <p class="message-text">{message.content}</p>
                  {/if}
                  {#each toolEventsForMessage(message.id) as ev (ev.id)}
                    <ToolCallBlock
                      name={ev.toolName}
                      status={toolStatus(ev)}
                      input={ev.input}
                      result={ev.result ?? undefined}
                    />
                  {/each}
                  {#each timelineAttention.byMessage[message.id] ?? [] as block (block.requestId)}
                    {@render attentionBlock(block)}
                  {/each}
                </div>
              {/each}
            </div>
          {/snippet}

          {#if isAssistant && groupHasUsage(group)}
            <!-- eslint-disable-next-line @typescript-eslint/no-unused-vars -->
            {#snippet footer()}
              <MessageMeta model={conversationModel ?? undefined} tokens={groupTokenTotal(group)} />
            {/snippet}
          {/if}
        </MessageBubble>
      {/each}

      {#snippet attentionBlock(block: AttentionBlock)}
        <div class="attention-slot">
          {#if block.kind === 'question'}
            <QuestionnaireBlock
              questions={block.questions}
              status={block.status === 'waiting' ? 'waiting' : 'resolved'}
              answers={resolvedAnswers(block)}
              onsubmit={(answers) => onRespondQuestion?.(block.requestId, answers)}
            />
          {:else if block.kind === 'plan'}
            <PlanApprovalBlock
              plan={block.plan}
              status={block.status === 'waiting'
                ? 'waiting'
                : block.status === 'approved'
                  ? 'resolved'
                  : 'rejected'}
              initialFeedback={block.feedback}
              onapprove={() => onRespondPlan?.(block.requestId, { action: 'approve' })}
              onreject={(feedback) =>
                onRespondPlan?.(block.requestId, { action: 'reject', feedback })}
            />
          {:else if block.kind === 'permission'}
            <ToolPermissionBlock
              tool={block.toolName}
              input={block.input}
              status={block.status === 'waiting'
                ? 'waiting'
                : block.status === 'granted'
                  ? 'granted'
                  : 'denied'}
              onrespond={(decision) => onRespondPermission?.(block.requestId, decision)}
            />
          {/if}
        </div>
      {/snippet}

      {#each timelineAttention.orphan as block (block.requestId)}
        {@render attentionBlock(block)}
      {/each}

      {#if isStreaming}
        <div class="streaming-hint">Assistant is working…</div>
      {/if}
    </div>
  </div>

  {#if !autoScroll && (messages.length > 0 || pendingAttention.length > 0)}
    <button
      type="button"
      class="scroll-to-latest"
      onclick={scrollToLatest}
      title="Scroll to latest"
    >
      <ArrowDown size={14} />
      <span>Latest</span>
    </button>
  {/if}
</div>

<style>
  .stream-wrapper {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }

  .message-stream {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 14px;
    overflow-y: auto;
    overflow-x: hidden;
    min-height: 0;
    scroll-behavior: smooth;
  }

  .stream-column {
    width: 100%;
    max-width: 600px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .message-body {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .message-segment {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  .message-segment + .message-segment {
    padding-top: 10px;
    border-top: 1px solid var(--c-border-subtle);
  }

  .message-text {
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    flex: 1;
    color: var(--c-text-muted);
    font-size: 13px;
    text-align: center;
    padding: 40px 20px;
  }

  .empty-state p {
    margin: 0;
    max-width: 360px;
  }

  .attention-slot {
    display: flex;
    flex-direction: column;
  }

  .streaming-hint {
    align-self: flex-start;
    padding: 4px 10px;
    font-size: 11.5px;
    color: var(--c-text-muted);
    font-style: italic;
  }

  .scroll-to-latest {
    position: absolute;
    bottom: 12px;
    right: 18px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 999px;
    border: 1px solid var(--c-border);
    background: var(--c-bg-elevated);
    color: var(--c-text);
    font-size: 11.5px;
    font-family: inherit;
    cursor: pointer;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.25);
  }

  .scroll-to-latest:hover {
    background: var(--c-hover);
  }
</style>
