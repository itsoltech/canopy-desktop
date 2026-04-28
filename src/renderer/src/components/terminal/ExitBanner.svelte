<script lang="ts">
  let {
    exitCode,
    onRestart,
  }: {
    exitCode: number | null
    onRestart: () => void
  } = $props()
</script>

<div class="absolute inset-0 flex items-center justify-center bg-scrim z-pane-overlay">
  <div class="flex flex-col items-center gap-3">
    <span class="text-text-secondary text-md">
      Process exited{exitCode !== null ? ` (code ${exitCode})` : ''}
    </span>
    <button
      class="px-4 py-1.5 border border-text-faint rounded-lg bg-active text-text text-sm font-inherit cursor-pointer transition-colors duration-fast hover:bg-hover-strong"
      onclick={onRestart}>Restart</button
    >
  </div>
</div>
