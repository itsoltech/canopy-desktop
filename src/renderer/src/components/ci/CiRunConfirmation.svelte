<script lang="ts">
  import CiRunTargetSummary from './CiRunTargetSummary.svelte'

  // The confirmation SCREEN shared by both run dialogs — it replaces the dialog's body the
  // way the parameters screen does, rather than being appended to it. One component because
  // the decision is the same one for both providers: "this is what is about to be sent".
  //
  // Parameter names and values are printed verbatim. Canopy does not know what any of them
  // MEAN: they belong to each server's own build configurations and workflows, so anything
  // keyed off a specific one would read correctly for a single installation and mislead for
  // every other.
  let {
    title,
    targetLabel,
    ref,
    refLabel,
    changed,
    total,
    noun = 'parameters',
  }: {
    /** What will run — the build configuration or workflow label. */
    title: string
    targetLabel: 'Build' | 'Workflow'
    /** The branch or tag it will run on. */
    ref: string
    refLabel: 'Branch' | 'Tag'
    changed: Array<{ name: string; value: string }>
    /** How many were submitted in total, so the count reads as a proportion. */
    total: number
    noun?: 'parameters' | 'inputs'
  } = $props()
</script>

<div class="flex flex-col gap-3">
  <CiRunTargetSummary {targetLabel} target={title} {refLabel} {ref} />

  <div class="flex flex-col gap-1">
    <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint">
      {noun === 'inputs' ? 'Inputs' : 'Parameters'}
    </span>
    {#if changed.length > 0}
      <span class="text-2xs text-text-faint">{changed.length} of {total} changed from defaults</span
      >
      <ul
        class="m-0 flex max-h-48 flex-col gap-0.5 overflow-y-auto rounded-md border border-border-subtle p-2 list-none"
      >
        {#each changed as entry (entry.name)}
          <li class="font-mono text-2xs text-text-secondary break-all">
            {entry.name} = {entry.value}
          </li>
        {/each}
      </ul>
    {:else}
      <span class="text-xs text-text-muted">
        {total === 0 ? `No ${noun}.` : `Nothing changed from defaults.`}
      </span>
    {/if}
  </div>
</div>
