<script lang="ts">
  import Avatar from '../atoms/Avatar.svelte'
  import RoleBadge from '../atoms/RoleBadge.svelte'
  import Timestamp from '../atoms/Timestamp.svelte'
  import ModelBadge from '../atoms/ModelBadge.svelte'

  type Role = 'user' | 'assistant' | 'tool' | 'system'

  interface Props {
    role: Role
    model?: string
    brand?: 'ClaudeAI' | 'OpenAI' | 'Gemini'
    userInitial?: string
    timestamp?: Date | string | number
    label?: string
  }

  let { role, model, brand, userInitial, timestamp, label }: Props = $props()
</script>

<header class="message-header">
  <Avatar {role} model={brand} initial={userInitial} size={26} />
  <RoleBadge {role} {label} />
  {#if model}
    <ModelBadge {model} />
  {/if}
  {#if timestamp !== undefined}
    <span class="spacer"></span>
    <Timestamp {timestamp} />
  {/if}
</header>

<style>
  .message-header {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 18px;
    font-family: inherit;
    font-size: 0.86em;
  }

  .message-header :global(.avatar) {
    display: none;
  }

  .spacer {
    flex: 1;
  }
</style>
