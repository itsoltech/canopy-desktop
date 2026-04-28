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
  <span
    class="inline-flex items-center justify-center flex-shrink-0"
    style:color={info.color}
    style:width="{size}px"
    style:height="{size}px"
  >
    <LucideComponent {size} />
  </span>
{:else}
  <span
    class="inline-flex items-center justify-center flex-shrink-0"
    style:width="{size}px"
    style:height="{size}px"
  >
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
