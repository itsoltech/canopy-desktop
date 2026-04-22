<script lang="ts">
  import type { Snippet } from 'svelte'

  type Role = 'user' | 'assistant' | 'tool' | 'system'

  interface Props {
    role: Role
    header?: Snippet
    body: Snippet
    footer?: Snippet
    actions?: Snippet
  }

  let { role, header, body, footer, actions }: Props = $props()
</script>

<article class="message-bubble {role}" data-role={role}>
  {#if header}
    <div class="bubble-header">{@render header()}</div>
  {/if}

  <div class="bubble-body">
    {@render body()}
  </div>

  {#if footer}
    <div class="bubble-footer">{@render footer()}</div>
  {/if}

  {#if actions}
    <div class="bubble-actions">{@render actions()}</div>
  {/if}
</article>

<style>
  .message-bubble {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 4px;
    align-self: stretch;
    padding: 2px 0;
    border-radius: 0;
    border: 0;
    background: transparent;
    color: var(--c-text);
    font-family: inherit;
    font-size: inherit;
    line-height: 1.45;
    max-width: 100%;
  }

  .message-bubble.user {
    padding: 7px 10px 7px 12px;
    background: color-mix(in srgb, var(--c-accent) 6%, transparent);
    border-left: 2px solid var(--c-accent);
  }

  .message-bubble.system {
    color: var(--c-text-secondary);
  }

  .bubble-body {
    display: flex;
    flex-direction: column;
    gap: 0;
    white-space: pre-wrap;
    word-break: break-word;
    -webkit-user-select: text;
    user-select: text;
    cursor: text;
  }

  .bubble-footer {
    display: flex;
    align-items: center;
  }

  .bubble-actions {
    position: absolute;
    top: -12px;
    right: 10px;
  }
</style>
