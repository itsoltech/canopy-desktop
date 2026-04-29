<script lang="ts">
  import { fileManagerLabel } from '../../../lib/platform'

  interface Props {
    x: number
    y: number
    onClose: () => void
    onReveal: () => void
    onCopyPath: () => void
    onRemove: () => void
  }

  let { x, y, onClose, onReveal, onCopyPath, onRemove }: Props = $props()
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="fixed inset-0 z-overlay" onclick={onClose}>
  <div
    class="fixed min-w-45 bg-bg-overlay border border-border rounded-md shadow-ctx p-1 z-popover"
    style="left: {x}px; top: {y}px"
    onclick={(e) => e.stopPropagation()}
  >
    <button
      class="block w-full px-3 py-1.5 border-0 rounded-sm bg-transparent text-text text-md font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover"
      onclick={onReveal}>{fileManagerLabel()}</button
    >
    <button
      class="block w-full px-3 py-1.5 border-0 rounded-sm bg-transparent text-text text-md font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover"
      onclick={onCopyPath}>Copy Path</button
    >
    <div class="h-px mx-2 my-1 bg-border-subtle"></div>
    <button
      class="block w-full px-3 py-1.5 border-0 rounded-sm bg-transparent text-danger-text text-md font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover"
      onclick={onRemove}>Remove from Recent</button
    >
  </div>
</div>
