<script lang="ts">
  import type { Snippet } from 'svelte'
  import { untrack } from 'svelte'
  import { ChevronRight } from '@lucide/svelte'

  let {
    title,
    sectionKey,
    borderTop = false,
    headerExtra,
    children,
  }: {
    title: string
    sectionKey: string
    borderTop?: boolean
    headerExtra?: Snippet
    children: Snippet
  } = $props()

  const storageKey = untrack(() => `canopy:sidebar:collapsed:${sectionKey}`)

  let collapsed = $state(localStorage.getItem(storageKey) === '1')

  function toggle(): void {
    collapsed = !collapsed
    localStorage.setItem(storageKey, collapsed ? '1' : '0')
  }
</script>

<section class="py-3" class:border-t={borderTop} class:border-border-subtle={borderTop}>
  <div class="flex items-center justify-between px-3 h-7 mb-1">
    <button
      class="flex items-center gap-1 shrink-0 bg-transparent border-0 py-1 -my-1 cursor-pointer text-inherit group"
      onclick={toggle}
      aria-expanded={!collapsed}
    >
      <span
        class="flex items-center text-text-faint group-hover:text-text-muted transition-transform duration-base ease-std"
        class:rotate-90={!collapsed}
      >
        <ChevronRight size={12} />
      </span>
      <h3
        class="text-2xs font-semibold tracking-caps-looser uppercase text-text-faint group-hover:text-text-muted leading-tight"
      >
        {title}
      </h3>
    </button>
    {#if headerExtra}
      <!-- The extra gets every pixel the title doesn't need (long branch names truncate against
           the section label instead of bleeding past the sidebar). -->
      <div class="flex-1 min-w-0 flex items-center justify-end gap-1 pl-2">
        {@render headerExtra()}
      </div>
    {/if}
  </div>
  <div
    class="grid transition-grid-rows motion-reduce:transition-none"
    class:grid-rows-open={!collapsed}
    class:grid-rows-closed={collapsed}
  >
    <div class="overflow-hidden">
      {@render children()}
    </div>
  </div>
</section>
