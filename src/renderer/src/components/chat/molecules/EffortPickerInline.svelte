<script lang="ts">
  import CustomSelect from '../../shared/CustomSelect.svelte'

  interface Props {
    /** Null represents "use SDK default / inherit from profile". */
    value: string | null
    onchange?: (value: string | null) => void
  }

  let { value, onchange }: Props = $props()

  const options = [
    { value: '', label: 'Default effort' },
    { value: 'low', label: 'Low' },
    { value: 'medium', label: 'Medium' },
    { value: 'high', label: 'High' },
    { value: 'max', label: 'Max' },
  ]

  function handleChange(next: string): void {
    onchange?.(next === '' ? null : next)
  }
</script>

<div class="effort-picker-inline">
  <CustomSelect
    value={value ?? ''}
    {options}
    onchange={handleChange}
    maxWidth="180px"
    compact
    maxHeight={260}
  />
</div>

<style>
  .effort-picker-inline {
    display: inline-flex;
    align-items: center;
    min-width: 0;
  }

  .effort-picker-inline :global(.trigger) {
    padding: 3px 8px;
    min-height: 22px;
    font-size: 11.5px;
    border-radius: 4px;
    background: transparent;
    border-color: var(--c-border-subtle);
    color: var(--c-text-secondary);
  }

  .effort-picker-inline :global(.trigger:hover) {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .effort-picker-inline :global(.trigger-label) {
    font-size: 11px;
  }
</style>
