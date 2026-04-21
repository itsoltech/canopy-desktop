<script lang="ts">
  import ToolIcon from '../../shared/ToolIcon.svelte'

  type Role = 'user' | 'assistant' | 'tool' | 'system'

  interface Props {
    role?: Role
    model?: 'ClaudeAI' | 'OpenAI' | 'Gemini'
    initial?: string
    size?: number
  }

  let { role = 'assistant', model, initial, size = 28 }: Props = $props()

  let glyphSize = $derived(Math.round(size * 0.6))
  let fontSize = $derived(Math.round(size * 0.42))
</script>

<span class="avatar {role}" style:width="{size}px" style:height="{size}px" aria-label={role}>
  {#if model}
    <ToolIcon icon={model} size={glyphSize} />
  {:else if initial}
    <span class="initial" style:font-size="{fontSize}px">
      {initial.charAt(0).toUpperCase()}
    </span>
  {:else}
    <span class="initial" style:font-size="{fontSize}px">
      {role === 'user' ? 'Y' : '?'}
    </span>
  {/if}
</span>

<style>
  .avatar {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    border-radius: 50%;
    background: var(--c-bg-elevated);
    border: 1px solid var(--c-border-subtle);
    color: var(--c-text-secondary);
    overflow: hidden;
  }

  .avatar.user {
    background: var(--c-accent-bg);
    border-color: var(--c-accent-muted);
    color: var(--c-accent-text);
  }

  .avatar.tool {
    background: color-mix(in srgb, var(--c-generate) 15%, transparent);
    border-color: color-mix(in srgb, var(--c-generate) 40%, transparent);
    color: var(--c-generate);
  }

  .avatar.system {
    background: color-mix(in srgb, var(--c-warning) 15%, transparent);
    border-color: color-mix(in srgb, var(--c-warning) 40%, transparent);
    color: var(--c-warning);
  }

  .initial {
    font-weight: 600;
    line-height: 1;
  }
</style>
