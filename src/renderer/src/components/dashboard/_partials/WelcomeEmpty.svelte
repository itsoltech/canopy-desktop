<script lang="ts">
  import { FolderOpen } from '@lucide/svelte'

  interface Props {
    modKey: string
    onOpenFolder: () => void
    onOpenFromPath: () => void
    openFolderBtnEl?: HTMLButtonElement
  }

  let { modKey, onOpenFolder, onOpenFromPath, openFolderBtnEl = $bindable() }: Props = $props()
</script>

<div class="flex flex-col items-center text-center gap-6 mt-16">
  <div class="flex flex-col items-center gap-1.5">
    <h1 class="text-3xl font-semibold tracking-caps uppercase text-text m-0 leading-tight">
      Canopy
    </h1>
    <p class="text-sm text-text-muted m-0">developer terminal workstation</p>
  </div>

  <div class="flex flex-col items-center gap-2 mt-4">
    <button
      bind:this={openFolderBtnEl}
      type="button"
      class="inline-flex items-center gap-2 px-4 h-9 rounded-md text-md font-medium font-inherit cursor-pointer border-0 outline-none bg-accent-bg text-accent-text transition-colors duration-fast hover:bg-accent-bg-hover focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-2"
      onclick={onOpenFolder}
    >
      <FolderOpen size={14} />
      Open Folder
      <kbd class="text-2xs text-accent-text/70 font-sans ml-1.5 select-none">
        {modKey}O
      </kbd>
    </button>
    <button
      type="button"
      class="px-3 h-7 rounded-md text-sm font-inherit cursor-pointer border-0 outline-none bg-transparent text-text-secondary transition-colors duration-fast hover:bg-hover focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-2"
      onclick={onOpenFromPath}
    >
      Open from Path…
    </button>
  </div>
</div>
