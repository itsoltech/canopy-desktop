<script lang="ts">
  interface Props {
    label?: string
  }

  let { label }: Props = $props()
</script>

{#if label}
  <div class="divider with-label" role="separator" aria-label={label}>
    <span class="line"></span>
    <span class="label">{label}</span>
    <span class="line"></span>
  </div>
{:else}
  <hr class="divider" />
{/if}

<style>
  .divider {
    border: none;
    height: 1px;
    background: var(--c-border-subtle);
    margin: 0;
    width: 100%;
  }

  .divider.with-label {
    display: flex;
    align-items: center;
    gap: 8px;
    height: auto;
    background: transparent;
  }

  .divider.with-label .line {
    flex: 1;
    height: 1px;
    background: var(--c-border-subtle);
  }

  .divider.with-label .label {
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--c-text-muted);
    white-space: nowrap;
  }
</style>
