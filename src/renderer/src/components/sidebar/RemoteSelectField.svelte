<script lang="ts">
  import { CircleQuestionMark } from '@lucide/svelte'
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
      <button
        type="button"
        class="inline-flex items-center border-0 bg-transparent p-0 text-text-muted cursor-help rounded-sm focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
        aria-label="{label} help"
      >
        <CircleQuestionMark size={11} />
      </button>
    </Tooltip>
  </span>
  <CustomSelect {value} {options} {groups} {onchange} />
</label>
