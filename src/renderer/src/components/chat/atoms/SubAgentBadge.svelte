<script lang="ts">
  import { Bot } from '@lucide/svelte'

  interface Props {
    type: string
    showIcon?: boolean
  }

  let { type, showIcon = true }: Props = $props()
</script>

<span class="sub-agent-badge">
  {#if showIcon}
    <Bot size={11} />
  {/if}
  <span class="label">{type}</span>
</span>

<style>
  .sub-agent-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--c-generate) 18%, transparent);
    border: 1px solid color-mix(in srgb, var(--c-generate) 40%, transparent);
    color: var(--c-generate);
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.02em;
    line-height: 1.2;
    white-space: nowrap;
  }

  .label {
    text-transform: none;
  }
</style>
