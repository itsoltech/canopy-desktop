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
    gap: 6px;
    padding: 10px 14px;
    border-radius: 8px;
    border: 1px solid var(--c-border-subtle);
    background: var(--c-bg-elevated);
    color: var(--c-text);
    font-size: 13.5px;
    line-height: 1.55;
    max-width: 100%;
  }

  .message-bubble.user {
    background: var(--c-accent-bg);
    border-color: var(--c-accent-muted);
    align-self: flex-end;
  }

  .message-bubble.assistant {
    background: var(--c-bg-elevated);
    border-color: var(--c-border-subtle);
  }

  .message-bubble.tool {
    background: transparent;
    border-color: var(--c-border-subtle);
    border-style: dashed;
  }

  .message-bubble.system {
    background: color-mix(in srgb, var(--c-warning) 8%, transparent);
    border-color: color-mix(in srgb, var(--c-warning) 30%, transparent);
  }

  .bubble-body {
    white-space: pre-wrap;
    word-break: break-word;
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
