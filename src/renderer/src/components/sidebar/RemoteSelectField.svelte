<script lang="ts">
  import { CircleHelp } from '@lucide/svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import Tooltip from '../shared/Tooltip.svelte'

  type SelectOption = { value: string; label: string }
  type SelectGroup = { label: string; options: SelectOption[] }

  let {
    label,
    tooltip,
    value,
    options,
    groups,
    onchange,
  }: {
    label: string
    tooltip: string
    value: string
    options?: SelectOption[]
    groups?: SelectGroup[]
    onchange: (value: string) => void
  } = $props()
</script>

<label class="flex flex-col gap-1">
  <span class="flex items-center gap-1 text-2xs uppercase tracking-caps-tight text-text-faint">
    {label}
    <Tooltip text={tooltip}>
      <span class="inline-flex items-center text-text-muted">
        <CircleHelp size={11} />
      </span>
    </Tooltip>
  </span>
  <CustomSelect {value} {options} {groups} {onchange} />
</label>
