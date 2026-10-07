<script lang="ts">
  import { updateState, installUpdate, dismissUpdate } from '../lib/stores/updateState.svelte'

  let state = $derived(updateState.status)
  let version = $derived(updateState.version)
  let percent = $derived(updateState.percent)
  let errorMessage = $derived(updateState.errorMessage)

  let visible = $derived(state !== 'idle' && !updateState.dismissed)
</script>

{#if visible}
  <div
    class="fixed bottom-4 right-4 flex items-center gap-2.5 px-4 py-2.5 bg-bg-overlay border border-border rounded-xl backdrop-blur-md z-banner text-md text-text max-w-100 shadow-banner"
    class:!border-danger-bg={state === 'error'}
    role="status"
  >
    {#if state === 'up-to-date'}
      <span class="truncate">You're up to date!</span>
    {:else if state === 'available'}
      <span class="truncate">Update v{version} available</span>
      <button
        class="px-2.5 py-1 border border-border rounded-md bg-transparent text-text text-sm cursor-pointer whitespace-nowrap hover:bg-active"
        onclick={dismissUpdate}>Dismiss</button
      >
    {:else if state === 'downloading'}
      <!-- The live region announces the state once; the per-percent text is exposed through the
           progress bar instead of being re-announced on every download tick. -->
      <span class="truncate">Downloading update… <span aria-hidden="true">{percent}%</span></span>
      <div
        class="flex-1 min-w-20 h-1 bg-hover-strong rounded-sm overflow-hidden"
        role="progressbar"
        aria-label="Update download"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent}
      >
        <div
          class="h-full bg-accent-text rounded-sm transition-all duration-300"
          style="width: {percent}%"
        ></div>
      </div>
    {:else if state === 'installing'}
      <span class="truncate">Installing update…</span>
    {:else if state === 'ready'}
      <span class="truncate">Update v{version} ready</span>
      <button
        class="px-2.5 py-1 border border-accent-muted rounded-md bg-accent-bg text-accent-text text-sm cursor-pointer whitespace-nowrap hover:bg-accent-bg-hover"
        onclick={installUpdate}>Restart Now</button
      >
      <button
        class="px-2.5 py-1 border border-border rounded-md bg-transparent text-text text-sm cursor-pointer whitespace-nowrap hover:bg-active"
        onclick={dismissUpdate}>Later</button
      >
    {:else if state === 'error'}
      <span class="truncate" title={errorMessage}>Update failed: {errorMessage}</span>
      <button
        class="px-2.5 py-1 border border-border rounded-md bg-transparent text-text text-sm cursor-pointer whitespace-nowrap hover:bg-active"
        onclick={dismissUpdate}>Dismiss</button
      >
    {/if}
  </div>
{/if}
