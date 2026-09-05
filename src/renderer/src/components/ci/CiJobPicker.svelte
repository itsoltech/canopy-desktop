<script lang="ts">
  import { SvelteMap } from 'svelte/reactivity'
  import { ciJobPickerCopy, type CiJobPickerProvider } from '../../lib/ci/jobPickerCopy'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import Tooltip from '../shared/Tooltip.svelte'

  // The per-repo job selection list of the CI configurator: every job the server
  // exposes, grouped by TeamCity project, with editable sidebar labels for the
  // ticked ones. Selection state lives in the parent (it is what Save writes), so
  // ALL mutations go back through callbacks — this component only reads `selected`.

  interface ServerBuildType {
    id: string
    name: string
    projectName: string
  }

  let {
    serverTypes,
    provider = 'teamcity',
    selected,
    onToggle,
    onLabelChange,
  }: {
    serverTypes: ServerBuildType[]
    provider?: CiJobPickerProvider
    selected: SvelteMap<string, string>
    onToggle: (bt: ServerBuildType) => void
    onLabelChange: (id: string, label: string) => void
  } = $props()

  let copy = $derived(ciJobPickerCopy(provider))

  let groupedTypes = $derived.by(() => {
    const groups = new SvelteMap<string, ServerBuildType[]>()
    for (const bt of serverTypes) {
      const key = bt.projectName || 'Other'
      const list = groups.get(key)
      if (list) list.push(bt)
      else groups.set(key, [bt])
    }
    return [...groups.entries()]
  })
</script>

{#if serverTypes.length === 0}
  <span class="text-xs text-text-faint">{copy.empty}</span>
{:else}
  <div class="flex flex-col gap-2">
    <p class="m-0 text-xs text-text-muted leading-snug">
      {copy.description} The selection is written to the git-tracked
      <code class="font-mono">.canopy/config.json</code>, so after you commit it the whole team gets
      {copy.sharedSelection}. Labels are editable and shown in the sidebar.
    </p>
    {#each groupedTypes as [project, types] (project)}
      <div class="flex flex-col gap-1">
        <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
          >{project}</span
        >
        {#each types as bt (bt.id)}
          <div class="flex items-center gap-2">
            <!-- Focus enters through the checkbox, so the full name is available to sighted
                 keyboard users without adding another tab stop. -->
            <Tooltip text={bt.id === bt.name ? bt.name : `${bt.name} · ${bt.id}`}>
              <label
                class="flex items-center gap-2 text-sm text-text-secondary cursor-pointer select-none min-w-0"
              >
                <CustomCheckbox checked={selected.has(bt.id)} onchange={() => onToggle(bt)} />
                <!-- The id rides along visibly: the configurator's warnings name IDS
                   (they are what the git-shared file stores), and there is no
                   search field — a hover-only title can't be scanned for. The id
                   gets its own non-shrinking cell so only the NAME truncates:
                   appended inside one truncate span, the id was the first thing
                   the ellipsis removed. -->
                <span class="truncate">{bt.name}</span>
                {#if bt.id !== bt.name}
                  <span class="font-mono text-2xs text-text-faint shrink-0 max-w-40 truncate"
                    >· {bt.id}</span
                  >
                {/if}
              </label>
            </Tooltip>
            {#if selected.has(bt.id)}
              <input
                class="flex-1 min-w-24 max-w-48 px-2 py-0.5 border border-border rounded-md bg-bg-input text-text text-xs font-inherit outline-none focus:border-focus-ring"
                aria-label={`Sidebar label for ${bt.name}`}
                value={selected.get(bt.id) ?? bt.name}
                oninput={(e) => onLabelChange(bt.id, e.currentTarget.value)}
                title="Label shown in the sidebar"
              />
            {/if}
          </div>
        {/each}
      </div>
    {/each}
  </div>
{/if}
