<script lang="ts">
  import Titlebar from './components/Titlebar.svelte'
  import MainLayout from './components/layout/MainLayout.svelte'
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
