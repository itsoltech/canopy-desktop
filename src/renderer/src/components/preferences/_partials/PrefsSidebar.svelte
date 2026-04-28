<script lang="ts">
  import type { Snippet } from 'svelte'
  import { sectionMeta, sectionMatches } from './sectionMeta'
  import { prefsSearch } from './prefsSearch.svelte'

  interface Group {
    readonly label: string
    readonly sections: readonly string[]
  }

  interface Props {
    groups: readonly Group[]
    activeSection: string
    onselect: (section: string) => void
    footer?: Snippet
  }

  let { groups, activeSection, onselect, footer }: Props = $props()
</script>

<aside
  class="w-52 shrink-0 flex flex-col border-r border-border-subtle bg-bg-overlay overflow-hidden"
>
  <div class="flex-1 overflow-y-auto py-3 px-2 select-none">
    {#each groups as group, gi (group.label)}
      <div role="group" aria-labelledby={`prefs-group-${group.label}`} class:mt-3={gi > 0}>
        <span
          id={`prefs-group-${group.label}`}
          class="block px-2 pb-1 text-2xs font-semibold uppercase tracking-caps-looser text-text-faint"
        >
          {group.label}
        </span>
        <div class="flex flex-col gap-px">
          {#each group.sections as section (section)}
            {@const meta = sectionMeta[section]}
            {@const Icon = meta?.icon}
            {@const dimmed =
              prefsSearch.query.trim() !== '' && !sectionMatches(section, prefsSearch.query)}
            {@const active = activeSection === section}
            <button
              type="button"
              aria-current={active ? 'page' : undefined}
              class="group relative flex items-center gap-2 w-full pl-3 pr-2 py-1.5 border-0 text-md font-inherit text-left cursor-pointer rounded-md outline-none focus-visible:ring-2 focus-visible:ring-focus-ring"
              class:bg-active={active}
              class:text-text={active}
              class:bg-transparent={!active}
              class:text-text-secondary={!active}
              class:hover:bg-hover={!active}
              class:hover:text-text={!active}
              class:opacity-40={dimmed && !active}
              onclick={() => onselect(section)}
            >
              {#if active}
                <span
                  aria-hidden="true"
                  class="absolute left-0 top-1/2 -translate-y-1/2 w-0.5 h-4 bg-accent rounded-r-sm"
                ></span>
              {/if}
              {#if Icon}
                <Icon size={14} class="shrink-0 {active ? 'text-accent' : 'text-text-muted'}" />
              {/if}
              <span class="truncate">{section}</span>
            </button>
          {/each}
        </div>
      </div>
    {/each}
  </div>

  {#if footer}
    <div class="border-t border-border-subtle px-2 py-2 shrink-0">
      {@render footer()}
    </div>
  {/if}
</aside>
