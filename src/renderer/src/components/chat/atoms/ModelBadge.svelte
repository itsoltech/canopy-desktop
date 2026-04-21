<script lang="ts">
  interface Props {
    model: string
  }

  let { model }: Props = $props()
</script>

<span class="model-badge" title={model}>{model}</span>

<style>
  .model-badge {
    display: inline-flex;
    align-items: center;
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 10.5px;
    font-variant-numeric: tabular-nums;
    color: var(--c-text-muted);
    letter-spacing: 0;
  }
</style>
