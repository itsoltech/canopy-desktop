<script lang="ts">
  const isMac = navigator.userAgent.includes('Mac')
</script>

<div class="titlebar" class:mac={isMac}>
  <span class="title">nixtty</span>
</div>

<style>
  .titlebar {
    height: 40px;
    app-region: drag;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    position: relative;
    user-select: none;
    background: rgba(30, 30, 30, 0.75);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  }

  .titlebar.mac {
    padding-left: 78px;
  }

  .title {
    font-size: 12px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.6);
    letter-spacing: 0.5px;
  }
</style>
