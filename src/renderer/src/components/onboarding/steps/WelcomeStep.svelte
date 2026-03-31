<script lang="ts">
  import { Layers } from '@lucide/svelte'
</script>

<div class="step">
  <div class="icon-wrap">
    <Layers size={48} strokeWidth={1.5} color="#74c0fc" />
  </div>
  <h2 class="title">Welcome to Canopy</h2>
  <p class="description">
    A developer workstation for managing multiple projects, branches, and AI assistants in parallel.
  </p>
  <p class="hint">
    This setup takes about a minute. You can change everything later in Preferences.
  </p>
</div>

<style>
  .step {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 12px;
    padding: 24px 0;
  }

  .icon-wrap {
    width: 80px;
    height: 80px;
    border-radius: 20px;
    background: rgba(116, 192, 252, 0.1);
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 8px;
  }

  .title {
    margin: 0;
    font-size: 22px;
    font-weight: 600;
    color: #e0e0e0;
  }

  .description {
    margin: 0;
    font-size: 14px;
    color: rgba(255, 255, 255, 0.6);
    line-height: 1.5;
    max-width: 380px;
  }

  .hint {
    margin: 8px 0 0;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.3);
  }
</style>
