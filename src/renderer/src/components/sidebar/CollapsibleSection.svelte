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

<section class="sidebar-section" class:border-top={borderTop}>
  <div class="section-header">
    <button class="section-toggle" onclick={toggle} aria-expanded={!collapsed}>
      <span class="chevron" class:open={!collapsed}>
        <ChevronRight size={12} />
      </span>
      <h3 class="section-title">{title}</h3>
    </button>
    {#if headerExtra}
      {@render headerExtra()}
    {/if}
  </div>
  <div class="section-body" class:collapsed>
    <div class="section-body-inner">
      {@render children()}
    </div>
  </div>
</section>

<style>
  .sidebar-section {
    padding: 12px 0;
  }

  .border-top {
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }

  .section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px 8px;
  }

  .section-toggle {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: 1;
    min-width: 0;
    background: none;
    border: none;
    padding: 4px 0;
    margin: -4px 0;
    cursor: pointer;
    color: inherit;
  }

  .section-title {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 1px;
    color: rgba(255, 255, 255, 0.4);
    text-transform: uppercase;
  }

  .section-toggle:hover .section-title {
    color: rgba(255, 255, 255, 0.6);
  }

  .chevron {
    display: flex;
    align-items: center;
    color: rgba(255, 255, 255, 0.3);
    transition: transform 0.15s ease;
    transform: rotate(0deg);
  }

  .chevron.open {
    transform: rotate(90deg);
  }

  .section-body {
    display: grid;
    grid-template-rows: 1fr;
    transition: grid-template-rows 0.15s ease;
  }

  .section-body.collapsed {
    grid-template-rows: 0fr;
  }

  .section-body-inner {
    overflow: hidden;
  }
</style>
