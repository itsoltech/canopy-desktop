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
    // Keep onMount synchronous so the cleanup returned by initUpdateListeners()
    // is registered as the teardown callback. An async onMount returns a
    // Promise, which Svelte ignores — leaking the update IPC listeners.
    void loadPrefs()
    return initUpdateListeners()
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
