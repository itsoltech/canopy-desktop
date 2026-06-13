<script lang="ts">
  let {
    statusDotStyle,
    statusLabel,
    hostLabel,
  }: {
    statusDotStyle: string
    statusLabel: string
    hostLabel: string | null
  } = $props()
</script>

<div class="flex items-center gap-2 min-w-0">
  <span class="size-2 rounded-full shrink-0" style={statusDotStyle} aria-hidden="true"></span>
  <div class="min-w-0 flex-1">
    <div class="text-sm text-text truncate" title={statusLabel}>{statusLabel}</div>
    {#if hostLabel}
      <div class="text-2xs text-text-muted font-mono truncate" title={hostLabel}>
        {hostLabel}
      </div>
    {:else}
      <div class="text-2xs text-text-muted truncate">No active listener</div>
    {/if}
  </div>
</div>
