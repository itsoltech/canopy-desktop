<script lang="ts">
  import { onMount } from 'svelte'
  import Titlebar from './components/Titlebar.svelte'
  import MainLayout from './components/layout/MainLayout.svelte'
  import StatusBar from './components/layout/StatusBar.svelte'
  import UpdateBanner from './components/UpdateBanner.svelte'
  import { loadPrefs, prefs } from './lib/stores/preferences.svelte'
  import { initUpdateListeners } from './lib/stores/updateState.svelte'
  import { applyAppTheme } from './lib/theme/appTheme'
  import { getTheme } from './lib/terminal/themes'

  onMount(() => {
    // Register update listeners synchronously and return their disposer so
    // Svelte actually runs it on unmount. An `async` onMount returns a Promise
    // (not the cleanup), so the IPC subscriptions would otherwise leak. Prefs
    // load in the background — the listeners don't depend on them.
    const disposeUpdateListeners = initUpdateListeners()
    void loadPrefs()
    return disposeUpdateListeners
  })

  $effect(() => {
    applyAppTheme(getTheme(prefs.theme || 'Default'))
  })
</script>

<div class="w-screen h-screen overflow-hidden flex flex-col">
  <Titlebar />
  <MainLayout />
  <StatusBar />
  <UpdateBanner />
</div>
