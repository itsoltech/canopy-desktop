<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import { ExternalLink, LoaderCircle, RefreshCw, X } from '@lucide/svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import { getCiActivityTick } from '../../lib/stores/ci.svelte'
  import { cycleFocus } from '../../lib/a11y/focusTrap'
  import { formatDuration, formatWhen } from '../../lib/ci/format'
  import { ciChip } from '../../lib/ci/status'
  import type { CiActivityBuild } from '../../lib/ci/types'

  // Server activity window: everything running and queued on the TeamCity server plus
  // recent history. The sidebar only carries a one-row summary — the details live
  // here, where there is room for them. Refreshes while open.

  let { repoRoot }: { repoRoot: string } = $props()

  let activity = $state<{
    running: CiActivityBuild[]
    queued: CiActivityBuild[]
    recent: CiActivityBuild[]
  } | null>(null)
  let error = $state('')
  let loaded = $state(false)
  let refreshing = $state(false)
  let now = $state(Date.now())
  let dialogEl = $state<HTMLElement>()
  let seq = 0

  async function refresh(): Promise<void> {
    // Mirrors the button's aria-disabled (which does not stop clicks); the seq
    // guard already makes overlap harmless — this keeps handler and attribute
    // in agreement.
    if (refreshing) return
    const mySeq = ++seq
    refreshing = true
    try {
      const result = await window.api.ciActivity(repoRoot)
      if (mySeq !== seq) return
      activity = result
      now = Date.now()
      error = ''
    } catch (e) {
      if (mySeq !== seq) return
      error = e instanceof Error ? e.message : 'Failed to load activity'
    } finally {
      if (mySeq === seq) {
        loaded = true
        refreshing = false
      }
    }
  }

  onMount(() => {
    dialogEl?.focus()
  })

  $effect(() => {
    // A trigger elsewhere in the app bumps the tick → refresh right away.
    void getCiActivityTick()
    untrack(() => void refresh())
    const timer = setInterval(() => void refresh(), 10_000)
    return () => clearInterval(timer)
  })

  function buildMeta(build: CiActivityBuild): string {
    if (build.state === 'finished') {
      const when = build.startedAt ?? build.finishedAt
      const parts: string[] = []
      if (when != null) parts.push(formatWhen(when, now))
      if (build.startedAt != null && build.finishedAt != null) {
        parts.push(formatDuration(build.finishedAt - build.startedAt))
      }
      return parts.join(' · ')
    }
    if (build.state === 'running') {
      const parts: string[] = []
      if (build.startedAt != null) {
        parts.push(formatWhen(build.startedAt, now))
        parts.push(`${formatDuration(now - build.startedAt)} elapsed`)
      }
      return parts.join(' · ')
    }
    return build.queuedAt != null ? `queued ${formatWhen(build.queuedAt, now)}` : ''
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      closeDialog()
      return
    }
    if (e.key === 'Tab' && dialogEl) cycleFocus(dialogEl, e)
  }

  function openBuild(webUrl: string): void {
    if (webUrl) window.api.openExternal(webUrl)
  }
</script>

{#snippet buildRow(build: CiActivityBuild)}
  {@const meta = buildMeta(build)}
  {@const chip = ciChip({ build })}
  <button
    type="button"
    class="group flex items-center gap-2.5 w-full min-h-8 px-3 py-1 border-0 bg-transparent text-text text-sm font-inherit text-left rounded-md transition-colors duration-fast enabled:cursor-pointer enabled:hover:bg-hover disabled:cursor-default"
    disabled={!build.webUrl}
    onclick={() => openBuild(build.webUrl)}
    title={build.webUrl ? 'Open in TeamCity' : undefined}
  >
    <span class="flex-1 min-w-0 flex flex-col items-start gap-0.5">
      <span class="w-full flex items-baseline gap-2 min-w-0">
        <span class="truncate">{build.buildTypeName}</span>
        {#if build.number}
          <span class="font-mono text-2xs text-text-faint flex-shrink-0">#{build.number}</span>
        {/if}
      </span>
      <span class="w-full flex items-baseline gap-2 min-w-0">
        {#if build.branchName}
          <span class="font-mono text-2xs text-text-muted truncate" title={build.branchName}
            >{build.branchName}</span
          >
        {/if}
        {#if meta}
          <span class="ml-auto text-2xs text-text-faint whitespace-nowrap flex-shrink-0"
            >{meta}</span
          >
        {/if}
      </span>
    </span>
    <span class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 {chip.cls}">{chip.label}</span>
    <ExternalLink
      size={11}
      class="shrink-0 opacity-0 transition-opacity duration-fast group-hover:opacity-60 group-focus-within:opacity-60"
    />
  </button>
{/snippet}

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-overlay flex justify-center items-center bg-scrim"
  onmousedown={closeDialog}
  onkeydown={handleKeydown}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- Native CSS resize (bottom-right handle): needs explicit dimensions and a
       non-visible overflow; the inner list scrolls independently. -->
  <div
    bind:this={dialogEl}
    class="outline-none w-[560px] h-[520px] min-w-[420px] min-h-[300px] max-w-[95vw] max-h-[92vh] flex flex-col bg-bg-overlay border border-border rounded-xl shadow-modal overflow-hidden"
    style="resize: both"
    role="dialog"
    aria-modal="true"
    aria-label="Jobs history"
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
    <header
      class="px-5 pt-4 pb-3 border-b border-border-subtle shrink-0 flex items-center justify-between gap-3"
    >
      <h2 class="text-base font-semibold text-text m-0 leading-tight">Jobs history</h2>
      <div class="flex items-center gap-1">
        <!-- aria-disabled, not disabled: `refreshing` flips on a 10 s TIMER, and a
             real disabled would blur a merely-focused user to <body>, past the
             focus trap on the descendant backdrop div. -->
        <button
          type="button"
          class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text shrink-0 aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-transparent aria-disabled:hover:text-text-muted"
          onclick={() => void refresh()}
          aria-disabled={refreshing}
          aria-busy={refreshing}
          aria-label="Refresh"
          title={refreshing ? 'Refreshing…' : 'Refresh now (auto-refreshes every 10 s)'}
        >
          <RefreshCw
            size={13}
            class={refreshing ? 'animate-spin-slow motion-reduce:animate-none' : ''}
          />
        </button>
        <button
          type="button"
          class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text shrink-0"
          onclick={closeDialog}
          aria-label="Close"
          title="Close"
        >
          <X size={16} />
        </button>
      </div>
    </header>

    <div class="flex-1 overflow-y-auto px-2 py-3 flex flex-col gap-1">
      {#if !loaded}
        <div class="flex items-center gap-2 px-3 py-2 text-sm text-text-faint">
          <LoaderCircle size={14} class="animate-spin-slow motion-reduce:animate-none" />
          Loading activity…
        </div>
      {:else if error}
        <p class="px-3 py-2 m-0 text-sm text-danger-text" title={error}>{error}</p>
      {:else if activity}
        <span
          class="px-3 pt-1 pb-0.5 text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
          >Running & queued</span
        >
        {#if activity.running.length === 0 && activity.queued.length === 0}
          <p class="px-3 py-1 m-0 text-sm text-text-faint">Nothing is running or queued.</p>
        {:else}
          {#each [...activity.running, ...activity.queued] as build (build.state + build.id)}
            {@render buildRow(build)}
          {/each}
        {/if}

        <span
          class="px-3 pt-3 pb-0.5 text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
          >Recent</span
        >
        {#if activity.recent.length === 0}
          <p class="px-3 py-1 m-0 text-sm text-text-faint">No finished builds yet.</p>
        {:else}
          {#each activity.recent as build (build.id)}
            {@render buildRow(build)}
          {/each}
        {/if}
      {/if}
    </div>
  </div>
</div>
