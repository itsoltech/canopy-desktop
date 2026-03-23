<script lang="ts">
  import { onMount } from 'svelte'
  import Titlebar from './components/Titlebar.svelte'
  import MainLayout from './components/layout/MainLayout.svelte'
  import { openWorkspace } from './lib/stores/workspace.svelte'
  import { loadPrefs } from './lib/stores/preferences.svelte'

  onMount(async () => {
    // Load preferences before anything else
    await loadPrefs()

    // Restore last workspace on cold start (unless disabled in preferences)
    const reopenPref = await window.api.getPref('reopenLastWorkspace')
    if (reopenPref !== 'false') {
      const lastPath = await window.api.getPref('lastWorkspacePath')
      if (lastPath) {
        openWorkspace(lastPath).catch(() => {
          // Workspace path no longer exists — ignore
        })
      }
    }
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
