<script lang="ts">
  import SlashToken from '../atoms/SlashToken.svelte'

  interface Props {
    command: string
    description?: string
    focused?: boolean
    onselect?: () => void
  }

  let { command, description, focused = false, onselect }: Props = $props()
</script>

<button class="slash-hint" class:focused type="button" onclick={() => onselect?.()}>
  <SlashToken {command} />
  {#if description}
    <span class="description">{description}</span>
  {/if}
</button>

<style>
  .slash-hint {
    display: flex;
    align-items: center;
    gap: 10px;
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

  .slash-hint:hover,
  .slash-hint.focused {
    background: var(--c-hover);
  }

  .slash-hint:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 2px var(--c-focus-ring);
  }

  .description {
    flex: 1;
    color: var(--c-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
