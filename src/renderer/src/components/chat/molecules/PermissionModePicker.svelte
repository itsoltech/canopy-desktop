<script lang="ts">
  import CustomSelect from '../../shared/CustomSelect.svelte'

  interface Option {
    value: string
    label: string
  }

  interface Props {
    value: string
    options: Option[]
    onchange?: (value: string) => void
  }

  let { value, options, onchange }: Props = $props()
</script>

<div class="permission-mode-picker">
  <CustomSelect {value} {options} {onchange} maxWidth="220px" />
</div>

<style>
  .permission-mode-picker {
    display: inline-flex;
    align-items: center;
    min-width: 0;
  }

  .permission-mode-picker :global(.trigger) {
    padding: 3px 8px;
    min-height: 22px;
    font-size: 11.5px;
    border-radius: 4px;
    background: transparent;
    border-color: var(--c-border-subtle);
    color: var(--c-text-secondary);
  }

  .permission-mode-picker :global(.trigger:hover) {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .permission-mode-picker :global(.trigger-label) {
    font-size: 11px;
  }
</style>
