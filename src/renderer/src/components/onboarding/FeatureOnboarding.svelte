<script lang="ts">
  import { onMount } from 'svelte'
  import { Sparkles } from '@lucide/svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import { onboardingState, finishOnboarding } from '../../lib/stores/onboarding.svelte'

  interface Props {
    fromVersion: string
  }

  let { fromVersion }: Props = $props()
  let containerEl: HTMLDivElement | undefined = $state()
  let submitting = $state(false)

  onMount(() => {
    containerEl?.focus()
  })

  async function handleDismiss(): Promise<void> {
    if (submitting) return
    submitting = true
    await finishOnboarding()
    closeDialog()
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape' || e.key === 'Enter') {
      e.preventDefault()
      handleDismiss()
    } else if (e.key === 'Tab' && containerEl) {
      const focusable = containerEl.querySelectorAll<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
      )
      if (focusable.length === 0) return
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault()
        last.focus()
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault()
        first.focus()
      }
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onkeydown={handleKeydown} onmousedown={handleDismiss}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={containerEl}
    class="container"
    role="dialog"
    aria-modal="true"
    aria-label="New features"
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
    <div class="header">
      <div class="icon-wrap">
        <Sparkles size={20} strokeWidth={1.5} color="var(--color-accent)" />
      </div>
      <div>
        <h2 class="title">New in this update</h2>
        <span class="subtitle">Since v{fromVersion}</span>
      </div>
    </div>

    <div class="features">
      {#each onboardingState.steps as step (step.id)}
        <div class="feature-card">
          <div class="feature-title">{step.title}</div>
          <div class="feature-desc">{step.description}</div>
        </div>
      {/each}
    </div>

    <div class="actions">
      <button class="btn-dismiss" onclick={handleDismiss}>Got it</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 1001;
    display: flex;
    justify-content: center;
    align-items: center;
    background: var(--color-scrim);
  }

  .container {
    outline: none;
    width: 440px;
    max-width: 90vw;
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    background: var(--color-bg-overlay);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    box-shadow: 0 16px 48px oklch(0 0 0 / 0.6);
    padding: 24px;
  }

  .header {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 20px;
  }

  .icon-wrap {
    width: 40px;
    height: 40px;
    border-radius: 10px;
    background: var(--color-accent-bg);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .title {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
    color: var(--color-text);
  }

  .subtitle {
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .features {
    display: flex;
    flex-direction: column;
    gap: 8px;
    overflow-y: auto;
    min-height: 0;
    flex: 1;
  }

  .feature-card {
    padding: 12px 14px;
    border-radius: 8px;
    background: var(--color-border-subtle);
    border: 1px solid var(--color-border-subtle);
  }

  .feature-title {
    font-size: 13px;
    font-weight: 500;
    color: var(--color-text);
    margin-bottom: 4px;
  }

  .feature-desc {
    font-size: 12px;
    color: var(--color-text-muted);
    line-height: 1.4;
  }

  .actions {
    display: flex;
    justify-content: center;
    margin-top: 20px;
    flex-shrink: 0;
  }

  .btn-dismiss {
    padding: 7px 24px;
    border-radius: 6px;
    font-size: 13px;
    font-family: inherit;
    font-weight: 500;
    cursor: pointer;
    border: none;
    outline: none;
    background: var(--color-accent-bg);
    color: var(--color-accent-text);
    transition: background 0.1s;
  }

  .btn-dismiss:hover {
    background: var(--color-accent-bg-hover);
  }

  .btn-dismiss:focus-visible {
    outline: 2px solid var(--color-focus-ring);
    outline-offset: 1px;
  }
</style>
