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
  import ToolSelectionStep from './steps/ToolSelectionStep.svelte'
  import EnvironmentCheckStep from './steps/EnvironmentCheckStep.svelte'
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

<div
  role="presentation"
  class="fixed inset-0 z-[1001] flex justify-center items-center bg-scrim"
  onkeydown={handleKeydown}
>
  <div
    bind:this={containerEl}
    class="outline-none w-[560px] max-w-[90vw] max-h-[85vh] flex flex-col bg-bg-overlay border border-border rounded-xl shadow-modal overflow-hidden"
    role="dialog"
    aria-modal="true"
    aria-label="Setup wizard"
    tabindex="-1"
  >
    <div class="flex-1 min-h-0 overflow-y-auto px-8 pt-8 pb-4">
      {#if step?.id === 'welcome'}
        <WelcomeStep />
      {:else if step?.id === 'tool-selection'}
        <ToolSelectionStep />
      {:else if step?.id === 'environment-check'}
        <EnvironmentCheckStep />
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

    <div class="flex-shrink-0 flex flex-col gap-4 px-8 pt-4 pb-6 border-t border-border-subtle">
      <StepIndicator total={onboardingState.steps.length} current={onboardingState.currentStep} />

      <div class="flex justify-between">
        {#if isFirst}
          <button
            class="px-5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none bg-transparent text-text-secondary transition-colors duration-fast hover:bg-hover hover:text-text focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
            onclick={handleSkip}>Skip setup</button
          >
        {:else}
          <button
            class="px-5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none bg-transparent text-text-secondary transition-colors duration-fast hover:bg-hover hover:text-text focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
            onclick={handleBack}>Back</button
          >
        {/if}

        {#if isLast}
          <button
            class="px-5 py-1.5 rounded-lg text-md font-medium font-inherit cursor-pointer border-0 outline-none bg-accent-bg text-accent-text transition-colors duration-fast hover:bg-accent-bg-hover focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
            onclick={handleFinish}>Done</button
          >
        {:else}
          <button
            class="px-5 py-1.5 rounded-lg text-md font-medium font-inherit cursor-pointer border-0 outline-none bg-accent-bg text-accent-text transition-colors duration-fast hover:bg-accent-bg-hover focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
            onclick={handleNext}>Next</button
          >
        {/if}
      </div>
    </div>
  </div>
</div>
