<script lang="ts">
  import { FileText } from '@lucide/svelte'

  interface Props {
    path: string
    focused?: boolean
    onselect?: () => void
  }

  let { path, focused = false, onselect }: Props = $props()

  let name = $derived(path.substring(path.lastIndexOf('/') + 1))
  let dir = $derived(path.includes('/') ? path.substring(0, path.lastIndexOf('/')) : '')
</script>

<button class="file-hint" class:focused type="button" onclick={() => onselect?.()}>
  <FileText size={12} class="icon" />
  <span class="name">{name}</span>
  {#if dir}
    <span class="dir">{dir}</span>
  {/if}
</button>

<style>
  .file-hint {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 5px 8px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--c-text);
    font-family: inherit;
    font-size: 12.5px;
    text-align: left;
    cursor: pointer;
    transition: background 0.1s;
  }

  .file-hint :global(.icon) {
    color: var(--c-text-muted);
    flex-shrink: 0;
  }

  .file-hint:hover,
  .file-hint.focused {
    background: var(--c-hover);
  }

  .file-hint:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 2px var(--c-focus-ring);
  }

  .name {
    color: var(--c-text);
    flex-shrink: 0;
    white-space: nowrap;
  }

  .dir {
    flex: 1;
    color: var(--c-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }
</style>
