<script lang="ts">
  import type { FileIconInfo } from '../../lib/fileIcons'

  interface Props {
    info: FileIconInfo
    size?: number
  }

  let { info, size = 14 }: Props = $props()
</script>

{#if info.kind === 'lucide'}
  {@const LucideComponent = info.Icon}
  <span class="file-icon-wrap" style:color={info.color} style:--file-icon-size="{size}px">
    <LucideComponent {size} />
  </span>
{:else}
  <span class="file-icon-wrap" style:--file-icon-size="{size}px">
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill={info.color}
      role="img"
      aria-label={info.title}
    >
      <path d={info.path} />
    </svg>
  </span>
{/if}

<style>
  .file-icon-wrap {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--file-icon-size, 14px);
    height: var(--file-icon-size, 14px);
    flex-shrink: 0;
  }
</style>
