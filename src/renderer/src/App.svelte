<script lang="ts">
  import { onMount } from 'svelte'
  import Titlebar from './components/Titlebar.svelte'
  import MainLayout from './components/layout/MainLayout.svelte'
  import { loadPrefs } from './lib/stores/preferences.svelte'

  onMount(async () => {
    // Load preferences before anything else
    await loadPrefs()
    // Workspace restore is handled by main process via url:action events
  })
</script>

<div class="app">
  <Titlebar />
  <MainLayout />
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
