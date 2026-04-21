<script lang="ts">
  interface Props {
    command: string
  }

  let { command }: Props = $props()

  let display = $derived(command.startsWith('/') ? command : `/${command}`)
</script>

<span class="slash-token">{display}</span>

<style>
  .slash-token {
    display: inline-flex;
    align-items: center;
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 11.5px;
    font-weight: 500;
    line-height: 1;
    padding: 2px 6px;
    border-radius: 3px;
    color: var(--c-accent-text);
    background: var(--c-accent-bg);
    border: 1px solid var(--c-accent-muted);
  }
</style>
