<script lang="ts">
  import { MessageSquarePlus, MessageSquare } from '@lucide/svelte'
  import type { Conversation as SdkConversation } from '../../../../../main/db/sdkAgentRows'

  interface Props {
    workspaceId: string
    /** Currently focused conversation — rendered in an active state. */
    activeConversationId?: string | null
    onOpen?: (conversationId: string) => void
    onNew?: () => void
  }

  let { workspaceId, activeConversationId = null, onOpen, onNew }: Props = $props()

  let conversations: SdkConversation[] = $state([])
  let loading = $state(true)

  async function refresh(): Promise<void> {
    loading = true
    try {
      conversations = await window.api.sdkAgent.list(workspaceId)
    } catch (e) {
      console.warn('[ConversationListSidebar] list failed', e)
      conversations = []
    } finally {
      loading = false
    }
  }

  $effect(() => {
    // Re-fetch when workspace changes.
    void workspaceId
    void refresh()
  })

  function prettyTitle(conv: SdkConversation): string {
    if (conv.title && conv.title.trim().length > 0) return conv.title
    return 'Untitled chat'
  }

  function relativeTime(iso: string): string {
    const diff = Date.now() - new Date(iso).getTime()
    const seconds = Math.floor(diff / 1000)
    if (seconds < 60) return 'just now'
    const minutes = Math.floor(seconds / 60)
    if (minutes < 60) return `${minutes}m ago`
    const hours = Math.floor(minutes / 60)
    if (hours < 24) return `${hours}h ago`
    const days = Math.floor(hours / 24)
    return `${days}d ago`
  }

  function handleNew(): void {
    onNew?.()
    // Caller creates the conversation; we refresh after a short delay so the
    // sidebar picks up the new row without forcing an explicit prop flow.
    setTimeout(() => void refresh(), 200)
  }
</script>

<aside class="conversation-sidebar" aria-label="Conversations">
  <header class="sidebar-head">
    <h2>Conversations</h2>
    <button type="button" class="new-chat" onclick={handleNew} title="New chat">
      <MessageSquarePlus size={14} />
      <span>New</span>
    </button>
  </header>

  <ul class="conversation-list">
    {#if loading}
      <li class="placeholder">Loading…</li>
    {:else if conversations.length === 0}
      <li class="placeholder">No conversations yet.</li>
    {:else}
      {#each conversations as conv (conv.id)}
        <li>
          <button
            type="button"
            class="conv-row"
            class:active={conv.id === activeConversationId}
            onclick={() => onOpen?.(conv.id)}
          >
            <MessageSquare size={13} />
            <div class="conv-body">
              <div class="conv-title">{prettyTitle(conv)}</div>
              <div class="conv-meta">{conv.model} · {relativeTime(conv.updatedAt)}</div>
            </div>
          </button>
        </li>
      {/each}
    {/if}
  </ul>
</aside>

<style>
  .conversation-sidebar {
    display: flex;
    flex-direction: column;
    width: 260px;
    min-width: 220px;
    max-width: 320px;
    border-right: 1px solid var(--c-border-subtle);
    background: var(--c-bg);
    height: 100%;
  }

  .sidebar-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
    border-bottom: 1px solid var(--c-border-subtle);
  }

  .sidebar-head h2 {
    margin: 0;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--c-text-muted);
    font-weight: 600;
  }

  .new-chat {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    background: transparent;
    color: var(--c-accent-text);
    border: 1px solid color-mix(in srgb, var(--c-accent) 40%, transparent);
    border-radius: 4px;
    font-size: 11.5px;
    font-family: inherit;
    cursor: pointer;
  }

  .new-chat:hover {
    background: var(--c-accent-bg);
  }

  .conversation-list {
    list-style: none;
    margin: 0;
    padding: 6px 0;
    overflow-y: auto;
  }

  .placeholder {
    padding: 12px;
    font-size: 12px;
    color: var(--c-text-muted);
    font-style: italic;
  }

  .conv-row {
    width: 100%;
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 12px;
    background: transparent;
    color: var(--c-text);
    border: 0;
    text-align: left;
    font-family: inherit;
    font-size: 12.5px;
    cursor: pointer;
  }

  .conv-row:hover {
    background: var(--c-hover);
  }

  .conv-row.active {
    background: var(--c-active);
  }

  .conv-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }

  .conv-title {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .conv-meta {
    font-size: 10.5px;
    color: var(--c-text-muted);
  }
</style>
