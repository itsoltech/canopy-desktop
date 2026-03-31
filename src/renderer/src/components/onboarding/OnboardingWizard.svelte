<script lang="ts">
  import { onMount } from 'svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import {
    onboardingState,
    currentStepDef,
    nextStep,
    prevStep,
    finishOnboarding,
    skipOnboarding,
  } from '../../lib/stores/onboarding.svelte'
  import { openWorkspace } from '../../lib/stores/workspace.svelte'
  import StepIndicator from './StepIndicator.svelte'
  import WelcomeStep from './steps/WelcomeStep.svelte'
  import ThemeStep from './steps/ThemeStep.svelte'
  import AiSetupStep from './steps/AiSetupStep.svelte'
  import FeaturesStep from './steps/FeaturesStep.svelte'
  import ReadyStep from './steps/ReadyStep.svelte'

  let containerEl: HTMLDivElement | undefined = $state()
  let submitting = $state(false)

  let step = $derived(currentStepDef())
  let isFirst = $derived(onboardingState.currentStep === 0)
  let isLast = $derived(onboardingState.currentStep === onboardingState.steps.length - 1)

  onMount(() => {
    containerEl?.focus()
  })

  async function handleNext(): Promise<void> {
    if (isLast) {
      await handleFinish()
    } else {
      await nextStep()
    }
  }

  function handleBack(): void {
    prevStep()
  }

  async function handleSkip(): Promise<void> {
    if (submitting) return
    submitting = true
    await skipOnboarding()
    closeDialog()
  }

  async function handleFinish(): Promise<void> {
    if (submitting) return
    submitting = true
    await finishOnboarding()
    closeDialog()
  }

  async function handleOpenFolder(): Promise<void> {
    if (submitting) return
    submitting = true
    await finishOnboarding()
    closeDialog()
    const path = await window.api.openFolder()
    if (path) openWorkspace(path)
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      handleSkip()
    } else if (e.key === 'Enter') {
      e.preventDefault()
      if (isLast) {
        handleFinish()
      } else {
        handleNext()
      }
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onkeydown={handleKeydown}>
  <div
    bind:this={containerEl}
    class="wizard"
    role="dialog"
    aria-modal="true"
    aria-label="Setup wizard"
    tabindex="-1"
  >
    <div class="wizard-content">
      {#if step?.id === 'welcome'}
        <WelcomeStep />
      {:else if step?.id === 'theme'}
        <ThemeStep />
      {:else if step?.id === 'ai-setup'}
        <AiSetupStep />
      {:else if step?.id === 'features'}
        <FeaturesStep />
      {:else if step?.id === 'ready'}
        <ReadyStep onOpenFolder={handleOpenFolder} />
      {/if}
    </div>

    <div class="wizard-footer">
      <StepIndicator total={onboardingState.steps.length} current={onboardingState.currentStep} />

      <div class="nav-buttons">
        {#if isFirst}
          <button class="btn btn-ghost" onclick={handleSkip}>Skip setup</button>
        {:else}
          <button class="btn btn-ghost" onclick={handleBack}>Back</button>
        {/if}

        {#if isLast}
          <button class="btn btn-primary" onclick={handleFinish}>Done</button>
        {:else}
          <button class="btn btn-primary" onclick={handleNext}>Next</button>
        {/if}
      </div>
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
    background: rgba(0, 0, 0, 0.6);
  }

  .wizard {
    outline: none;
    width: 560px;
    max-width: 90vw;
    max-height: 85vh;
    display: flex;
    flex-direction: column;
    background: rgba(30, 30, 30, 0.98);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 12px;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.6);
    overflow: hidden;
  }

  .wizard-content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 32px 32px 16px;
  }

  .wizard-footer {
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 16px 32px 24px;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }

  .nav-buttons {
    display: flex;
    justify-content: space-between;
  }

  .btn {
    padding: 7px 20px;
    border-radius: 6px;
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
    border: none;
    outline: none;
    transition: background 0.1s;
  }

  .btn:focus-visible {
    outline: 2px solid rgba(116, 192, 252, 0.6);
    outline-offset: 1px;
  }

  .btn-ghost {
    background: transparent;
    color: rgba(255, 255, 255, 0.5);
  }

  .btn-ghost:hover {
    background: rgba(255, 255, 255, 0.06);
    color: rgba(255, 255, 255, 0.7);
  }

  .btn-primary {
    background: rgba(116, 192, 252, 0.2);
    color: rgba(116, 192, 252, 0.95);
    font-weight: 500;
  }

  .btn-primary:hover {
    background: rgba(116, 192, 252, 0.3);
  }
</style>
