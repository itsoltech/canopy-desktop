<script lang="ts">
  import { FileText, Image, File } from '@lucide/svelte'
  import Chip from '../atoms/Chip.svelte'

  interface Props {
    name: string
    sizeBytes?: number
    kind?: 'file' | 'image' | 'text'
    onremove?: () => void
  }

  let { name, sizeBytes, kind = 'file', onremove }: Props = $props()

  let sizeLabel = $derived.by(() => {
    if (sizeBytes === undefined) return null
    if (sizeBytes < 1024) return `${sizeBytes} B`
    if (sizeBytes < 1024 * 1024) return `${(sizeBytes / 1024).toFixed(0)} KB`
    return `${(sizeBytes / (1024 * 1024)).toFixed(1)} MB`
  })
</script>

<Chip {onremove}>
  {#snippet icon()}
    {#if kind === 'image'}
      <Image size={12} />
    {:else if kind === 'text'}
      <FileText size={12} />
    {:else}
      <File size={12} />
    {/if}
  {/snippet}
  <span class="attachment-name">{name}</span>
  {#if sizeLabel}
    <span class="attachment-size">{sizeLabel}</span>
  {/if}
</Chip>

<style>
  .attachment-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 160px;
  }

  .attachment-size {
    margin-left: 4px;
    color: var(--c-text-faint);
    font-size: 10.5px;
    font-variant-numeric: tabular-nums;
  }
</style>
