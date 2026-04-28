<script lang="ts">
  let {
    tmuxSessionName,
    onReattach,
    onKill,
  }: {
    tmuxSessionName: string
    onReattach: () => void
    onKill: () => void
  } = $props()
</script>

<div class="absolute inset-0 flex items-center justify-center bg-scrim z-pane-overlay">
  <div class="flex flex-col items-center gap-2">
    <span class="text-text text-lg font-medium">Session detached</span>
    <span class="text-text-faint text-xs font-mono">{tmuxSessionName}</span>
    <span class="text-text-secondary text-sm mb-1">Still running in background</span>
    <div class="flex gap-2">
      <button
        class="px-4 py-1.5 border border-accent rounded-lg bg-accent text-accent-text text-sm font-inherit cursor-pointer transition-colors duration-fast hover:opacity-90"
        onclick={onReattach}>Reattach</button
      >
      <button
        class="px-4 py-1.5 border border-text-faint rounded-lg bg-active text-text text-sm font-inherit cursor-pointer transition-colors duration-fast hover:bg-hover-strong hover:border-danger hover:text-danger"
        onclick={onKill}>Kill Session</button
      >
    </div>
  </div>
</div>
