<script lang="ts">
  import { ArrowUp, Square } from '@lucide/svelte'
  import IconButton from '../atoms/IconButton.svelte'
  import Kbd from '../atoms/Kbd.svelte'

  interface Props {
    onsend?: () => void
    onstop?: () => void
    disabled?: boolean
    stopping?: boolean
    showHint?: boolean
  }

  let { onsend, onstop, disabled = false, stopping = false, showHint = true }: Props = $props()
</script>

<div class="send-control">
  {#if showHint && !disabled && !stopping}
    <span class="hint">
      <Kbd>⌘</Kbd>
      <Kbd>↵</Kbd>
    </span>
  {/if}
  {#if stopping}
    <IconButton
      onclick={() => onstop?.()}
      tooltip="Stop"
      label="Stop generation"
      variant="danger"
      size="md"
    >
      <Square size={11} fill="currentColor" strokeWidth={2.5} />
    </IconButton>
  {:else}
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
  {/if}
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
