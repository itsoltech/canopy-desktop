<script lang="ts">
  import { ArrowUp } from '@lucide/svelte'
  import IconButton from '../atoms/IconButton.svelte'
  import Kbd from '../atoms/Kbd.svelte'

  interface Props {
    onsend?: () => void
    disabled?: boolean
    showHint?: boolean
  }

  let { onsend, disabled = false, showHint = true }: Props = $props()
</script>

<div class="send-control">
  {#if showHint && !disabled}
    <span class="hint">
      <Kbd>⌘</Kbd>
      <Kbd>↵</Kbd>
    </span>
  {/if}
  <IconButton
    onclick={() => onsend?.()}
    {disabled}
    tooltip="Send"
    label="Send message"
    variant="primary"
    size="md"
  >
    <ArrowUp size={14} strokeWidth={2.5} />
  </IconButton>
</div>

<style>
  .send-control {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .hint {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }
</style>
