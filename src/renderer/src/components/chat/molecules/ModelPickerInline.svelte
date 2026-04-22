<script lang="ts">
  import CustomSelect from '../../shared/CustomSelect.svelte'

  interface Option {
    value: string
    label: string
  }

  interface OptionGroup {
    label: string
    options: Option[]
  }

  interface Props {
    value: string
    options?: Option[]
    groups?: OptionGroup[]
    onchange?: (value: string) => void
  }

  let { value, options, groups, onchange }: Props = $props()
</script>

<div class="model-picker-inline">
  <CustomSelect {value} {options} {groups} {onchange} maxWidth="220px" compact maxHeight={420} />
</div>

<style>
  .model-picker-inline {
    display: inline-flex;
    align-items: center;
    min-width: 0;
  }

  .model-picker-inline :global(.trigger) {
    padding: 3px 8px;
    min-height: 22px;
    font-size: 11.5px;
    border-radius: 4px;
    background: transparent;
    border-color: var(--c-border-subtle);
    color: var(--c-text-secondary);
  }

  .model-picker-inline :global(.trigger:hover) {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .model-picker-inline :global(.trigger-label) {
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 11px;
  }
</style>
