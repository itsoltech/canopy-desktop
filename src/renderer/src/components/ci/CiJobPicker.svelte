<script lang="ts">
  import { SvelteMap } from 'svelte/reactivity'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'

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
    selected,
    onToggle,
    onLabelChange,
  }: {
    serverTypes: ServerBuildType[]
    selected: SvelteMap<string, string>
    onToggle: (bt: ServerBuildType) => void
    onLabelChange: (id: string, label: string) => void
  } = $props()

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
  <span class="text-xs text-text-faint">The server exposes no jobs.</span>
{:else}
  <div class="flex flex-col gap-2">
    <p class="m-0 text-xs text-text-muted leading-snug">
      These are all the jobs (build configurations) the TeamCity server exposes. Check the ones that
      belong to THIS repository — only those appear in Canopy (the CI/CD section, Run job and the
      branch context menu). The selection is written to the git-tracked
      <code class="font-mono">.canopy/config.json</code>, so after you commit it the whole team gets
      the same jobs. Labels are editable and shown in the sidebar.
    </p>
    {#each groupedTypes as [project, types] (project)}
      <div class="flex flex-col gap-1">
        <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
          >{project}</span
        >
        {#each types as bt (bt.id)}
          <div class="flex items-center gap-2">
            <label
              class="flex items-center gap-2 text-sm text-text-secondary cursor-pointer select-none min-w-0"
            >
              <CustomCheckbox checked={selected.has(bt.id)} onchange={() => onToggle(bt)} />
              <!-- The id rides along visibly: the configurator's warnings name IDS
                   (they are what the git-shared file stores), and there is no
                   search field — a hover-only title can't be scanned for. -->
              <span class="truncate" title={bt.id}
                >{bt.name}{#if bt.id !== bt.name}
                  <span class="font-mono text-2xs text-text-faint">· {bt.id}</span>{/if}</span
              >
            </label>
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
