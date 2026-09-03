<script lang="ts">
  import { Plus, Settings } from '@lucide/svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'
  import CiConfiguredContent from './_partials/CiConfiguredContent.svelte'
  import { createCiSectionState } from './ciSectionState.svelte'

  const state = createCiSectionState()
</script>

<span class="sr-only" aria-live="polite">{state.ciAnnouncement}</span>
<CollapsibleSection title="CI/CD" sectionKey="cicd" borderTop>
  {#snippet headerExtra()}
    <button
      class="flex items-center justify-center size-5 rounded-md border-0 bg-transparent text-text-faint cursor-pointer opacity-60 hover:opacity-100 hover:bg-hover hover:text-text-secondary"
      onclick={() => state.openConfigurator()}
      aria-label="Configure CI/CD"
      title={state.configureActionTitle}
    >
      <Settings size={12} />
    </button>
  {/snippet}

  <div
    bind:this={state.ciBodyEl}
    class="flex flex-col {state.ciBusy ? 'overflow-hidden' : ''}"
    style:height={state.ciBusy && state.ciFrozenHeight > 0
      ? `${state.ciFrozenHeight}px`
      : undefined}
  >
    {#if state.ciBusy && state.ciFrozenHeight > 0}
      <div class="flex flex-col" aria-hidden="true">
        {#each { length: state.ciPlaceholderRows }, index (index)}
          <div class="flex items-center h-7 px-3">
            <span
              class="h-2 rounded-sm bg-active animate-pulse motion-reduce:animate-none"
              style:width={`${[58, 40, 70, 48][index % 4]}%`}
            ></span>
          </div>
        {/each}
      </div>
    {:else if state.repoRoot && state.cfgState.loaded && state.config}
      <CiConfiguredContent {state} />
    {:else if state.repoRoot && state.cfgState.loaded}
      {#if state.cfgState.error}
        <div class="px-3 py-1 flex flex-col gap-0.5 text-xs text-warning-text">
          <span class="truncate" title={state.cfgState.error}>{state.cfgState.error}</span>
          <button
            type="button"
            class="self-start text-2xs underline underline-offset-2 bg-transparent border-0 p-0 font-inherit text-warning-text cursor-pointer hover:text-text"
            onclick={() => state.openConfigurator()}
          >
            Open the configurator
          </button>
        </div>
      {:else}
        <div class="px-3 py-2">
          <button
            class="flex items-center gap-1.5 w-full px-2.5 py-1.5 border border-dashed border-border rounded-lg bg-transparent text-text-muted text-sm font-inherit cursor-pointer transition-colors duration-fast hover:border-accent-muted hover:text-accent-text"
            onclick={() => state.openConfigurator()}
          >
            <Plus size={14} />
            Configure CI/CD
          </button>
        </div>
      {/if}
    {/if}
  </div>
</CollapsibleSection>
