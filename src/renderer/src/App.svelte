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

  onMount(async () => {
    // Load preferences before anything else
    await loadPrefs()
    // Workspace restore is handled by main process via url:action events
    return initUpdateListeners()
  })

  $effect(() => {
    applyAppTheme(getTheme(prefs.theme || 'Default'))
  })
</script>

<div class="app">
  <Titlebar />
  <MainLayout />
  <StatusBar />
  <UpdateBanner />
</div>

<style>
  .app {
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
</style>
