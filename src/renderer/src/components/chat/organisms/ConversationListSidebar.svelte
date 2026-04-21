<script lang="ts">
  import { MessageSquarePlus, MessageSquare, Search, X } from '@lucide/svelte'
  import type { Conversation as SdkConversation } from '../../../../../main/db/sdkAgentRows'
  import type { ConversationSearchHit } from '../../../../../main/db/ConversationStore'

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
  let query = $state('')
  let searchHits: ConversationSearchHit[] = $state([])
  let searching = $state(false)
  let searchDebounce: ReturnType<typeof setTimeout> | null = null

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

  function onQueryInput(e: Event): void {
    query = (e.currentTarget as HTMLInputElement).value
    if (searchDebounce !== null) clearTimeout(searchDebounce)
    const trimmed = query.trim()
    if (trimmed.length === 0) {
      searchHits = []
      searching = false
      return
    }
    searching = true
    searchDebounce = setTimeout(() => void runSearch(trimmed), 200)
  }

  async function runSearch(q: string): Promise<void> {
    try {
      searchHits = await window.api.sdkAgent.search({ workspaceId, query: q, limit: 25 })
    } catch (err) {
      console.warn('[ConversationListSidebar] search failed', err)
      searchHits = []
    } finally {
      searching = false
    }
  }

  function clearQuery(): void {
    query = ''
    searchHits = []
    searching = false
    if (searchDebounce !== null) clearTimeout(searchDebounce)
  }

  /**
   * Dangerously render an FTS5 snippet. The snippet string is wrapped in
   * <mark></mark> by SQLite — we trust that output because it originates
   * from our own rows and is not user-controlled HTML. All other
   * characters are escaped.
   */
  function renderSnippet(s: string): string {
    return s
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/&lt;mark&gt;/g, '<mark>')
      .replace(/&lt;\/mark&gt;/g, '</mark>')
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

  <div class="search-row">
    <Search size={12} />
    <input
      class="search-input"
      type="search"
      placeholder="Search messages…"
      value={query}
      oninput={onQueryInput}
      aria-label="Search conversations"
    />
    {#if query}
      <button type="button" class="clear-btn" onclick={clearQuery} title="Clear">
        <X size={12} />
      </button>
    {/if}
  </div>

  {#if query.trim().length > 0}
    <ul class="conversation-list">
      {#if searching}
        <li class="placeholder">Searching…</li>
      {:else if searchHits.length === 0}
        <li class="placeholder">No matches.</li>
      {:else}
        {#each searchHits as hit (hit.conversationId + hit.createdAt)}
          <li>
            <button
              type="button"
              class="conv-row hit"
              class:active={hit.conversationId === activeConversationId}
              onclick={() => onOpen?.(hit.conversationId)}
            >
              <MessageSquare size={13} />
              <div class="conv-body">
                <div class="conv-title">{hit.title ?? 'Untitled chat'}</div>
                <!-- eslint-disable-next-line svelte/no-at-html-tags -- renderSnippet escapes all HTML before reintroducing only <mark> tags produced by FTS5. -->
                <div class="conv-snippet">{@html renderSnippet(hit.snippet)}</div>
              </div>
            </button>
          </li>
        {/each}
      {/if}
    </ul>
  {:else}
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
  {/if}
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

  .search-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--c-border-subtle);
    color: var(--c-text-muted);
  }

  .search-input {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: 0;
    outline: none;
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
  }

  .clear-btn {
    display: inline-flex;
    background: transparent;
    color: var(--c-text-muted);
    border: 0;
    padding: 2px;
    border-radius: 3px;
    cursor: pointer;
  }

  .clear-btn:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .conv-row.hit .conv-body {
    gap: 4px;
  }

  .conv-snippet {
    font-size: 11px;
    color: var(--c-text-muted);
    line-height: 1.4;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
  }

  .conv-snippet :global(mark) {
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
    border-radius: 2px;
    padding: 0 2px;
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
