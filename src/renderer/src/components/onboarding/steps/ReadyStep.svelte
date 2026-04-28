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
    <Check size={48} strokeWidth={1.5} color="var(--color-success)" />
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
    background: var(--color-accent-bg);
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 8px;
  }

  .title {
    margin: 0;
    font-size: 22px;
    font-weight: 600;
    color: var(--color-text);
  }

  .description {
    margin: 0;
    font-size: 14px;
    color: var(--color-text-secondary);
    line-height: 1.5;
    max-width: 360px;
  }

  .description kbd {
    display: inline-block;
    padding: 1px 5px;
    font-size: 12px;
    font-family: inherit;
    background: var(--color-active);
    border: 1px solid var(--color-text-faint);
    border-radius: 4px;
    color: var(--color-text-secondary);
  }

  .open-btn {
    margin-top: 8px;
    padding: 10px 28px;
    border: none;
    border-radius: 8px;
    background: var(--color-accent-bg);
    color: var(--color-accent-text);
    font-size: 14px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    transition: background 0.15s;
  }

  .open-btn:hover {
    background: var(--color-accent-bg-hover);
  }

  .open-btn:focus-visible {
    outline: 2px solid var(--color-focus-ring);
    outline-offset: 2px;
  }
</style>
