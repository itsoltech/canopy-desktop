<script lang="ts">
  import { Check } from '@lucide/svelte'

  interface Props {
    onOpenFolder: () => void
  }

  let { onOpenFolder }: Props = $props()

  const isMac = navigator.userAgent.includes('Mac')
</script>

<div class="step">
  <div class="icon-wrap">
    <Check size={48} strokeWidth={1.5} color="#69db7c" />
  </div>
  <h2 class="title">You're all set</h2>
  <p class="description">
    Open a project folder to get started. You can always change your settings with
    <kbd>{isMac ? 'Cmd' : 'Ctrl'}+,</kbd>
  </p>

  <button class="open-btn" onclick={onOpenFolder}> Open folder </button>
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
    background: rgba(105, 219, 124, 0.1);
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
    color: rgba(255, 255, 255, 0.5);
    line-height: 1.5;
    max-width: 360px;
  }

  .description kbd {
    display: inline-block;
    padding: 1px 5px;
    font-size: 12px;
    font-family: inherit;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 4px;
    color: rgba(255, 255, 255, 0.6);
  }

  .open-btn {
    margin-top: 8px;
    padding: 10px 28px;
    border: none;
    border-radius: 8px;
    background: rgba(116, 192, 252, 0.2);
    color: rgba(116, 192, 252, 0.95);
    font-size: 14px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    transition: background 0.15s;
  }

  .open-btn:hover {
    background: rgba(116, 192, 252, 0.3);
  }

  .open-btn:focus-visible {
    outline: 2px solid rgba(116, 192, 252, 0.6);
    outline-offset: 2px;
  }
</style>
