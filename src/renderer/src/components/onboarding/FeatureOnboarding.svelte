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
<div
  class="fixed inset-0 z-[1001] flex justify-center items-center bg-scrim"
  onkeydown={handleKeydown}
  onmousedown={handleDismiss}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={containerEl}
    class="outline-none w-[440px] max-w-[90vw] max-h-[70vh] flex flex-col bg-bg-overlay border border-border rounded-[10px] shadow-modal p-6"
    role="dialog"
    aria-modal="true"
    aria-label="New features"
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
    <div class="flex items-center gap-3 mb-5">
      <div
        class="w-10 h-10 rounded-[10px] bg-accent-bg flex items-center justify-center flex-shrink-0"
      >
        <Sparkles size={20} strokeWidth={1.5} color="var(--color-accent)" />
      </div>
      <div>
        <h2 class="m-0 text-[17px] font-semibold text-text">New in this update</h2>
        <span class="text-sm text-text-muted">Since v{fromVersion}</span>
      </div>
    </div>

    <div class="flex flex-col gap-2 overflow-y-auto min-h-0 flex-1">
      {#each onboardingState.steps as step (step.id)}
        <div class="px-3.5 py-3 rounded-xl bg-border-subtle border border-border-subtle">
          <div class="text-md font-medium text-text mb-1">{step.title}</div>
          <div class="text-sm text-text-muted leading-snug">{step.description}</div>
        </div>
      {/each}
    </div>

    <div class="flex justify-center mt-5 flex-shrink-0">
      <button
        class="px-6 py-1.5 rounded-lg text-md font-medium font-inherit cursor-pointer border-0 outline-none bg-accent-bg text-accent-text transition-colors duration-fast hover:bg-accent-bg-hover focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
        onclick={handleDismiss}>Got it</button
      >
    </div>
  </div>
</div>
