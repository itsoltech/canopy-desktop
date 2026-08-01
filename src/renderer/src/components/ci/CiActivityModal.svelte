<script lang="ts">
  import { onMount } from 'svelte'
  import { ExternalLink, LoaderCircle, RefreshCw, X } from '@lucide/svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'

  // Server activity window: everything running and queued on the TeamCity server plus
  // recent history. The sidebar only carries a one-row summary — the details live
  // here, where there is room for them. Refreshes while open.

  let { repoRoot }: { repoRoot: string } = $props()

  interface ActivityBuild {
    id: number
    number: string | undefined
    state: 'running' | 'queued' | 'finished'
    status: string | undefined
    percentageComplete: number | undefined
    webUrl: string
    branchName: string | undefined
    buildTypeId: string
    buildTypeName: string
  }

  let activity = $state<{
    running: ActivityBuild[]
    queued: ActivityBuild[]
    recent: ActivityBuild[]
  } | null>(null)
  let error = $state('')
  let loaded = $state(false)
  let refreshing = $state(false)
  let dialogEl = $state<HTMLElement>()
  let seq = 0

  async function refresh(): Promise<void> {
    const mySeq = ++seq
    refreshing = true
    try {
      const result = await window.api.ciActivity(repoRoot)
      if (mySeq !== seq) return
      activity = result
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
    void refresh()
    const timer = setInterval(() => void refresh(), 10_000)
    return () => clearInterval(timer)
  })

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      closeDialog()
    }
  }

  function openBuild(webUrl: string): void {
    if (webUrl) window.api.openExternal(webUrl)
  }
</script>

{#snippet buildRow(build: ActivityBuild)}
  <button
    type="button"
    class="group flex items-center gap-2.5 w-full min-h-8 px-3 py-1 border-0 bg-transparent text-text text-sm font-inherit text-left rounded-md transition-colors duration-fast enabled:cursor-pointer enabled:hover:bg-hover disabled:cursor-default"
    disabled={!build.webUrl}
    onclick={() => openBuild(build.webUrl)}
    title={build.webUrl ? 'Open in TeamCity' : undefined}
  >
    <span class="flex-1 min-w-0 flex flex-col items-start gap-0.5">
      <span class="w-full truncate"
        >{build.buildTypeName}{#if build.number}<span class="text-text-faint">
            #{build.number}</span
          >{/if}</span
      >
      {#if build.branchName}
        <span class="w-full font-mono text-2xs text-text-muted truncate" title={build.branchName}
          >{build.branchName}</span
        >
      {/if}
    </span>
    {#if build.state === 'running'}
      <span class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 bg-accent-bg text-accent-text"
        >{build.percentageComplete != null
          ? `Running ${build.percentageComplete}%`
          : 'Running'}</span
      >
    {:else if build.state === 'queued'}
      <span class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 bg-active text-text-muted"
        >Queued</span
      >
    {:else if build.status === 'SUCCESS'}
      <span class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 bg-success-bg text-success-text"
        >Success</span
      >
    {:else if build.status === 'FAILURE'}
      <span class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 bg-danger-bg text-danger-text"
        >Failed</span
      >
    {:else}
      <span class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 bg-active text-text-muted"
        >{build.status ?? 'Unknown'}</span
      >
    {/if}
    <ExternalLink
      size={11}
      class="shrink-0 opacity-0 transition-opacity duration-fast group-hover:opacity-60"
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
  <div
    bind:this={dialogEl}
    class="outline-none w-[560px] max-w-[92vw] max-h-[80vh] flex flex-col bg-bg-overlay border border-border rounded-xl shadow-modal overflow-hidden"
    role="dialog"
    aria-modal="true"
    aria-label="CI activity"
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
    <header
      class="px-5 pt-4 pb-3 border-b border-border-subtle shrink-0 flex items-center justify-between gap-3"
    >
      <h2 class="text-base font-semibold text-text m-0 leading-tight">CI activity</h2>
      <div class="flex items-center gap-1">
        <button
          type="button"
          class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text shrink-0 disabled:opacity-50"
          onclick={() => void refresh()}
          disabled={refreshing}
          aria-label="Refresh"
          title="Refresh now (auto-refreshes every 10 s)"
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
