<script lang="ts">
  import { onMount } from 'svelte'
  import Titlebar from './components/Titlebar.svelte'
  import MainLayout from './components/layout/MainLayout.svelte'
  import StatusBar from './components/layout/StatusBar.svelte'
  import UpdateBanner from './components/UpdateBanner.svelte'
  import { loadPrefs } from './lib/stores/preferences.svelte'
  import { initUpdateListeners } from './lib/stores/updateState.svelte'

  onMount(async () => {
    // Load preferences before anything else
    await loadPrefs()
    // Workspace restore is handled by main process via url:action events
    return initUpdateListeners()
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
