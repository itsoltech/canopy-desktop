<script lang="ts">
  import type { Snippet } from 'svelte'
  import { SvelteMap } from 'svelte/reactivity'
  import { match } from 'ts-pattern'
  import { ArrowDown } from '@lucide/svelte'
  import MessageBubble from '../molecules/MessageBubble.svelte'
  import MessageHeader from '../molecules/MessageHeader.svelte'
  import MessageMeta from '../molecules/MessageMeta.svelte'
  import MarkdownContent from '../molecules/MarkdownContent.svelte'
  import ToolCallBlock from '../molecules/ToolCallBlock.svelte'
  import SubAgentRun from '../molecules/SubAgentRun.svelte'
  import ThinkingBlock from '../molecules/ThinkingBlock.svelte'
  import StreamingMarkdownContent from '../molecules/StreamingMarkdownContent.svelte'
  import TypingDots from '../atoms/TypingDots.svelte'
  import QuestionnaireBlock from '../molecules/QuestionnaireBlock.svelte'
  import PlanApprovalBlock from '../molecules/PlanApprovalBlock.svelte'
  import ToolPermissionBlock from '../molecules/ToolPermissionBlock.svelte'
  import type {
    AttentionBlock,
    SdkMessageView,
    SdkSubagentView,
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
    subagents?: Record<string, SdkSubagentView>
    pendingAttention: AttentionBlock[]
    isStreaming?: boolean
    composer?: Snippet
    composerKey?: string
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
    subagents = {},
    pendingAttention,
    isStreaming = false,
    composer,
    composerKey = '',
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

  interface ConversationTurn {
    id: string
    groups: MessageGroup[]
  }

  type PlanAttentionBlock = Extract<AttentionBlock, { kind: 'plan' }>

  interface ImageContentBlock {
    type: 'image'
    source: {
      type: 'base64'
      media_type: string
      data: string
    }
    filename?: string
  }

  function resolveBrand(model: string | null | undefined): Brand | undefined {
    if (!model) return undefined
    const m = model.toLowerCase()
    if (m.startsWith('claude') || m === 'opus' || m === 'sonnet' || m === 'haiku') return 'ClaudeAI'
    if (m.startsWith('gpt') || m.startsWith('o1') || m.startsWith('o3') || m === 'openai')
      return 'OpenAI'
    if (m.startsWith('gemini')) return 'Gemini'
    return undefined
  }

  let containerEl: HTMLDivElement | undefined = $state()
  let composerSlotEl: HTMLDivElement | undefined = $state()
  let autoScroll = $state(true)
  let scrollFrame: number | null = null
  let turnMinHeight = $state(360)
  let homeDir = $state('')
  let thinkingLengths = new SvelteMap<string, number>()
  let thinkingUpdatedAt = new SvelteMap<string, number>()
  let thinkingClock = $state(Date.now())
  let thinkingClockTimer: ReturnType<typeof setInterval> | null = null

  $effect(() => {
    let cancelled = false
    window.api
      .getHomedir()
      .then((home) => {
        if (!cancelled) homeDir = home
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  })

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
  let lastContentLength = $derived(
    (messages[messages.length - 1]?.content.length ?? 0) +
      (messages[messages.length - 1]?.thinking.length ?? 0),
  )
  $effect(() => {
    void lastContentLength
    scheduleScrollToBottom()
  })

  let toolActivitySignature = $derived.by(() =>
    Object.values(toolEvents)
      .map((event) => `${event.id}:${event.status}:${event.result?.length ?? 0}`)
      .join('|'),
  )
  $effect(() => {
    void toolActivitySignature
    scheduleScrollToBottom()
  })

  let subagentActivitySignature = $derived.by(() =>
    Object.values(subagents)
      .map((subagent) => subagentActivityKey(subagent))
      .join('|'),
  )
  $effect(() => {
    void subagentActivitySignature
    scheduleScrollToBottom()
  })

  $effect(() => {
    void composerKey
    scheduleScrollToBottom()
  })

  $effect(() => {
    return () => {
      if (scrollFrame !== null) cancelAnimationFrame(scrollFrame)
      if (thinkingClockTimer !== null) clearInterval(thinkingClockTimer)
    }
  })

  $effect(() => {
    const nextLengths = new SvelteMap<string, number>()
    const nextUpdatedAt = new SvelteMap<string, number>()
    const now = Date.now()

    for (const message of messages) {
      if (message.role !== 'assistant') continue
      const length = message.thinking.length
      if (length <= 0) continue
      nextLengths.set(message.id, length)
      const previousLength = thinkingLengths.get(message.id) ?? 0
      nextUpdatedAt.set(
        message.id,
        length > previousLength ? now : (thinkingUpdatedAt.get(message.id) ?? 0),
      )
    }

    for (const subagent of Object.values(subagents)) {
      for (const message of subagent.messages) {
        const key = subagentMessageKey(subagent.id, message.id)
        const length = message.thinking.length
        if (length <= 0) continue
        nextLengths.set(key, length)
        const previousLength = thinkingLengths.get(key) ?? 0
        nextUpdatedAt.set(key, length > previousLength ? now : (thinkingUpdatedAt.get(key) ?? 0))
      }
    }

    thinkingLengths = nextLengths
    thinkingUpdatedAt = nextUpdatedAt

    const hasRecentThinking = [...nextUpdatedAt.values()].some((updatedAt) => now - updatedAt < 900)
    if (hasRecentThinking && thinkingClockTimer === null) {
      thinkingClockTimer = setInterval(() => {
        thinkingClock = Date.now()
      }, 120)
    } else if (!hasRecentThinking && thinkingClockTimer !== null) {
      clearInterval(thinkingClockTimer)
      thinkingClockTimer = null
    }
  })

  $effect(() => {
    if (!containerEl) return

    const updateTurnHeight = (): void => {
      const composerHeight = composerSlotEl?.offsetHeight ?? 0
      turnMinHeight = Math.max(280, containerEl.clientHeight - composerHeight - 18)
    }

    updateTurnHeight()
    const observer = new ResizeObserver(updateTurnHeight)
    observer.observe(containerEl)
    if (composerSlotEl) observer.observe(composerSlotEl)

    return () => observer.disconnect()
  })

  function toolEventsForMessage(messageId: string): SdkToolEventView[] {
    return Object.values(toolEvents).filter(
      (t) => t.messageId === messageId && !isHiddenToolEvent(t),
    )
  }

  function isHiddenToolEvent(event: SdkToolEventView): boolean {
    return event.toolName === 'AskUserQuestion' || event.toolName === 'ToolSearch'
  }

  function planTextFromToolInput(input: Record<string, unknown>): string {
    return typeof input.plan === 'string' && input.plan.trim().length > 0
      ? input.plan
      : 'No plan text provided.'
  }

  function planPromptsFromToolInput(
    input: Record<string, unknown>,
  ): PlanAttentionBlock['allowedPrompts'] {
    const raw = input.allowedPrompts ?? input.allowed_prompts
    if (!Array.isArray(raw)) return []

    return raw.flatMap((item) => {
      if (!item || typeof item !== 'object') return []
      const record = item as Record<string, unknown>
      if (typeof record.tool !== 'string' || typeof record.prompt !== 'string') return []
      return [{ tool: record.tool, prompt: record.prompt }]
    })
  }

  function planStatusForTool(ev: SdkToolEventView): 'submitting' | 'approved' | 'rejected' {
    if (ev.status === 'running') return 'submitting'
    return ev.status === 'error' ? 'rejected' : 'approved'
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
        (question) => question.answers ?? ({} as Record<string, SdkAskUserQuestionAnswer>),
      )
      .otherwise(() => undefined)
  }

  function bubbleRole(message: SdkMessageView): BubbleRole {
    return message.role === 'tool' ? 'tool' : message.role
  }

  function headerRole(group: MessageGroup): BubbleRole {
    if (group.role !== 'system') return group.role === 'user' ? 'user' : 'assistant'
    return 'system'
  }

  function headerLabel(group: MessageGroup): string | undefined {
    if (group.role !== 'system') return undefined
    return group.messages.some((message) => /interrupted|aborted|cancelled/i.test(message.content))
      ? 'Warning'
      : 'System'
  }

  function groupHasUsage(group: MessageGroup): boolean {
    return group.messages.some((message) => message.tokensIn !== null || message.tokensOut !== null)
  }

  function groupHasFooter(group: MessageGroup): boolean {
    return groupHasUsage(group) || groupElapsedMs(group) !== null
  }

  function groupTokensIn(group: MessageGroup): number | undefined {
    for (let i = group.messages.length - 1; i >= 0; i--) {
      const value = group.messages[i].tokensIn
      if (value !== null) return value
    }
    return undefined
  }

  function groupTokensOut(group: MessageGroup): number | undefined {
    for (let i = group.messages.length - 1; i >= 0; i--) {
      const value = group.messages[i].tokensOut
      if (value !== null) return value
    }
    return undefined
  }

  function groupElapsedMs(group: MessageGroup): number | null {
    for (const message of group.messages) {
      if (message.elapsedMs !== null) return message.elapsedMs
    }
    return null
  }

  function groupCostUsd(group: MessageGroup): number | undefined {
    for (let i = group.messages.length - 1; i >= 0; i--) {
      const value = group.messages[i].costUsd
      if (value !== null) return value
    }
    return undefined
  }

  function groupModel(group: MessageGroup): string | null {
    for (const message of group.messages) {
      if (message.model !== null) return message.model
    }
    return null
  }

  function imageBlocks(message: SdkMessageView): ImageContentBlock[] {
    return message.contentBlocks.filter(isImageContentBlock)
  }

  function isImageContentBlock(block: Record<string, unknown>): block is ImageContentBlock {
    const source = block.source as Record<string, unknown> | undefined
    return (
      block.type === 'image' &&
      source?.type === 'base64' &&
      typeof source.media_type === 'string' &&
      typeof source.data === 'string'
    )
  }

  function imageSrc(block: ImageContentBlock): string {
    return `data:${block.source.media_type};base64,${block.source.data}`
  }

  function escapeRegExp(value: string): string {
    return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  }

  function redactHome(value: string): string {
    if (!homeDir) return value
    const normalizedHome = homeDir.replace(/\\/g, '/').replace(/\/+$/, '')
    const variants = new Set([homeDir.replace(/[/\\]+$/, ''), normalizedHome])
    let redacted = value
    for (const variant of variants) {
      if (!variant) continue
      redacted = redacted.replace(new RegExp(`${escapeRegExp(variant)}(?=$|[/\\\\])`, 'g'), '~')
    }
    return redacted
  }

  function subagentActivityKey(subagent: SdkSubagentView): string {
    const last = subagent.messages[subagent.messages.length - 1]
    const tools = Object.values(subagent.toolEvents)
      .map((event) => `${event.id}:${event.status}:${event.result?.length ?? 0}`)
      .join(',')
    return `${subagent.id}:${subagent.status}:${subagent.messages.length}:${last?.content.length ?? 0}:${last?.thinking.length ?? 0}:${tools}`
  }

  function subagentMessageKey(subagentId: string, messageId: string): string {
    return `${subagentId}:${messageId}`
  }

  function thinkingActive(messageId: string): boolean {
    const updatedAt = thinkingUpdatedAt.get(messageId)
    return updatedAt !== undefined && thinkingClock - updatedAt < 700
  }

  let latestAssistantHasText = $derived.by(() => {
    for (let i = messages.length - 1; i >= 0; i--) {
      const message = messages[i]
      if (message.role !== 'assistant') continue
      return message.content.trim().length > 0
    }
    return false
  })

  let latestAssistantHasThinking = $derived.by(() => {
    for (let i = messages.length - 1; i >= 0; i--) {
      const message = messages[i]
      if (message.role !== 'assistant') continue
      return message.thinking.trim().length > 0
    }
    return false
  })

  let showTypingIndicator = $derived(isStreaming && !latestAssistantHasText)
  let showIdleThinkingIndicator = $derived(showTypingIndicator && !latestAssistantHasThinking)

  let timelineAttention = $derived.by(() => {
    const byMessage: Record<string, AttentionBlock[]> = {}
    const byTool: Record<string, AttentionBlock> = {}
    const orphan: AttentionBlock[] = []

    for (const block of pendingAttention) {
      // Hide resolved permission blocks — the tool call itself records the
      // outcome, so leaving the banner around just clutters the transcript.
      if (block.kind === 'permission' && block.status !== 'waiting') continue
      if (block.kind === 'plan' && block.toolEventId && toolEvents[block.toolEventId]) {
        byTool[block.toolEventId] = block
        continue
      }
      const messageId = block.messageId
      const hasMessage = messageId ? messages.some((message) => message.id === messageId) : false
      if (!messageId || !hasMessage) {
        orphan.push(block)
        continue
      }
      byMessage[messageId] = [...(byMessage[messageId] ?? []), block]
    }

    return { byMessage, byTool, orphan }
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

  let messageTurns = $derived.by<ConversationTurn[]>(() => {
    const turns: ConversationTurn[] = []
    let current: ConversationTurn | null = null

    for (const group of messageGroups) {
      if (group.role === 'user' || !current) {
        current = {
          id: `turn-${group.id}`,
          groups: [],
        }
        turns.push(current)
      }
      current.groups = [...current.groups, group]
    }

    return turns
  })

  let latestTurnId = $derived(messageTurns[messageTurns.length - 1]?.id ?? null)
</script>

<div class="stream-wrapper">
  <div class="message-stream" bind:this={containerEl} onscroll={onScroll}>
    <div class="stream-column">
      {#if messages.length === 0 && pendingAttention.length === 0}
        <div class="empty-state">
          <p>Start a conversation. The assistant is ready to help with this worktree.</p>
        </div>
      {/if}

      {#each messageTurns as turn (turn.id)}
        <section
          class="message-turn"
          class:latest-turn={turn.id === latestTurnId}
          style={`--turn-min-height: ${turnMinHeight}px`}
        >
          {#each turn.groups as group (group.id)}
            {@render messageGroup(group)}
          {/each}

          {#if turn.id === latestTurnId}
            {#each timelineAttention.orphan as block (block.requestId)}
              {@render attentionBlock(block)}
            {/each}

            {@render typingIndicator()}
          {/if}
        </section>
      {/each}

      {#if messageTurns.length === 0}
        <section class="message-turn latest-turn" style={`--turn-min-height: ${turnMinHeight}px`}>
          {#each timelineAttention.orphan as block (block.requestId)}
            {@render attentionBlock(block)}
          {/each}

          {@render typingIndicator()}
        </section>
      {/if}

      {#snippet messageGroup(group: MessageGroup)}
        {@const firstMessage = group.messages[0]}
        {@const isAssistant = group.role === 'assistant'}
        {@const groupModelName = isAssistant
          ? (groupModel(group) ?? conversationModel ?? undefined)
          : undefined}
        <MessageBubble role={group.role}>
          {#snippet header()}
            <MessageHeader
              role={headerRole(group)}
              label={headerLabel(group)}
              brand={isAssistant ? resolveBrand(groupModelName) : undefined}
              model={groupModelName}
              userInitial={group.role === 'user' ? 'Y' : undefined}
              timestamp={firstMessage.createdAt}
            />
          {/snippet}

          {#snippet body()}
            <div class="message-body">
              {#each group.messages as message (message.id)}
                <div class="message-segment">
                  {#if message.role === 'assistant' && message.thinking}
                    <ThinkingBlock
                      content={message.thinking}
                      active={thinkingActive(message.id)}
                      defaultOpen={false}
                    />
                  {/if}
                  {#if message.content}
                    {#if message.role === 'assistant'}
                      <StreamingMarkdownContent
                        content={message.content}
                        active={isStreaming && message.id === messages[messages.length - 1]?.id}
                        onreveal={scheduleScrollToBottom}
                      />
                    {:else}
                      <p class="message-text">{message.content}</p>
                    {/if}
                  {/if}
                  {#each imageBlocks(message) as block, index (`${message.id}-image-${index}`)}
                    <figure class="image-attachment">
                      <img src={imageSrc(block)} alt={block.filename ?? 'Attached image'} />
                      {#if block.filename}
                        <figcaption>{block.filename}</figcaption>
                      {/if}
                    </figure>
                  {/each}
                  {#each toolEventsForMessage(message.id) as ev (ev.id)}
                    {#if subagents[ev.id]}
                      {@render subagentRun(subagents[ev.id], ev)}
                    {:else if ev.toolName === 'ExitPlanMode'}
                      {@render exitPlanModeBlock(ev)}
                    {:else}
                      <ToolCallBlock
                        name={ev.toolName}
                        status={toolStatus(ev)}
                        input={ev.input}
                        result={ev.result ?? undefined}
                      />
                    {/if}
                  {/each}
                  {#each timelineAttention.byMessage[message.id] ?? [] as block (block.requestId)}
                    {@render attentionBlock(block)}
                  {/each}
                </div>
              {/each}
            </div>
          {/snippet}

          {#if isAssistant && groupHasFooter(group)}
            <!-- eslint-disable-next-line @typescript-eslint/no-unused-vars -->
            {#snippet footer()}
              <MessageMeta
                model={groupModelName}
                tokensIn={groupTokensIn(group)}
                tokensOut={groupTokensOut(group)}
                costUsd={groupCostUsd(group)}
                elapsedMs={groupElapsedMs(group) ?? undefined}
              />
            {/snippet}
          {/if}
        </MessageBubble>
      {/snippet}

      {#snippet subagentRun(subagent: SdkSubagentView, parentEv: SdkToolEventView)}
        <SubAgentRun
          agentType={subagent.agentType}
          task={redactHome(subagent.task)}
          status={subagent.status}
          defaultOpen={false}
          activityKey={subagentActivityKey(subagent)}
          hasSummary={Boolean(parentEv.result)}
        >
          {#snippet body()}
            <div class="subagent-body">
              {#each subagent.messages as message (message.id)}
                {#if message.thinking}
                  <ThinkingBlock
                    content={message.thinking}
                    active={thinkingActive(subagentMessageKey(subagent.id, message.id))}
                    defaultOpen={false}
                  />
                {/if}
                {#if message.content}
                  <StreamingMarkdownContent
                    content={message.content}
                    active={subagent.status === 'running' &&
                      message.id === subagent.messages[subagent.messages.length - 1]?.id}
                    onreveal={scheduleScrollToBottom}
                  />
                {/if}
                {#each Object.values(subagent.toolEvents).filter((t) => t.messageId === message.id && !isHiddenToolEvent(t)) as nestedEv (nestedEv.id)}
                  <ToolCallBlock
                    name={nestedEv.toolName}
                    status={toolStatus(nestedEv)}
                    input={nestedEv.input}
                    result={nestedEv.result ?? undefined}
                  />
                {/each}
              {/each}
              {#if subagent.messages.length === 0 && Object.keys(subagent.toolEvents).length === 0}
                <div class="subagent-empty">Sub-agent is starting…</div>
              {/if}
            </div>
          {/snippet}
          {#snippet summary()}
            {#if parentEv.result}
              <MarkdownContent content={redactHome(parentEv.result)} />
            {/if}
          {/snippet}
        </SubAgentRun>
      {/snippet}

      {#snippet exitPlanModeBlock(ev: SdkToolEventView)}
        {@const block = timelineAttention.byTool[ev.id]}
        {#if block}
          {@render attentionBlock(block)}
        {:else}
          <PlanApprovalBlock
            status={planStatusForTool(ev)}
            allowedPrompts={planPromptsFromToolInput(ev.input) ?? []}
          >
            {#snippet plan()}
              <MarkdownContent content={planTextFromToolInput(ev.input)} />
            {/snippet}
          </PlanApprovalBlock>
        {/if}
      {/snippet}

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
              status={block.status}
              feedback={block.feedback}
              allowedPrompts={block.allowedPrompts ?? []}
              onapprove={() => onRespondPlan?.(block.requestId, { action: 'approve' })}
              onreject={(feedback) =>
                onRespondPlan?.(block.requestId, { action: 'reject', feedback })}
            >
              {#snippet plan()}
                <MarkdownContent content={block.plan} />
              {/snippet}
            </PlanApprovalBlock>
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

      {#snippet typingIndicator()}
        {#if showIdleThinkingIndicator}
          <div class="typing-row" aria-live="polite">
            <TypingDots label="Thinking" />
          </div>
        {:else if isStreaming}
          <div class="typing-row" aria-live="polite">
            <TypingDots label="Streaming" />
          </div>
        {/if}
      {/snippet}

      {#if composer}
        <div class="composer-slot" bind:this={composerSlotEl}>
          {@render composer()}
        </div>
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
    padding: 10px 18px;
    overflow-y: auto;
    overflow-x: hidden;
    min-height: 0;
    scroll-behavior: smooth;
    font-family: inherit;
    font-size: inherit;
    background: color-mix(in srgb, var(--c-bg) 96%, black);
  }

  .stream-column {
    width: 100%;
    max-width: 1120px;
    min-height: 100%;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .composer-slot {
    margin-top: auto;
    padding-top: 8px;
  }

  .message-turn {
    width: 100%;
    display: flex;
    flex-direction: column;
    flex: 0 0 auto;
    gap: 6px;
    padding-bottom: 10px;
  }

  .message-turn.latest-turn {
    min-height: var(--turn-min-height, 360px);
  }

  .message-body {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .message-segment {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .message-segment + .message-segment {
    padding-top: 6px;
  }

  .message-text {
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .image-attachment {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
    width: min(100%, 420px);
  }

  .image-attachment img {
    display: block;
    max-width: 100%;
    max-height: 320px;
    object-fit: contain;
    border-radius: 6px;
    border: 1px solid var(--c-border-subtle);
    background: var(--c-bg);
  }

  .image-attachment figcaption {
    color: var(--c-text-muted);
    font-size: 11px;
    line-height: 1.3;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    flex: 1;
    color: var(--c-text-muted);
    font-size: 0.95em;
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

  .subagent-body {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .subagent-empty {
    color: var(--c-text-muted);
    font-size: 0.9em;
    font-style: italic;
  }

  .typing-row {
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    min-height: 24px;
    padding: 4px 10px;
    color: var(--c-text-muted);
    font-size: 0.92em;
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
    font-size: 0.88em;
    font-family: inherit;
    cursor: pointer;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.25);
  }

  .scroll-to-latest:hover {
    background: var(--c-hover);
  }
</style>
