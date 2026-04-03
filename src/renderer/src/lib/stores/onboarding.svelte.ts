import { SvelteSet } from 'svelte/reactivity'
import { getFirstLaunchSteps, getFeatureSteps, type OnboardingStep } from '../onboarding/steps'

function semverGt(a: string, b: string): boolean {
  const pa = a.split('.').map(Number)
  const pb = b.split('.').map(Number)
  for (let i = 0; i < 3; i++) {
    if ((pa[i] ?? 0) > (pb[i] ?? 0)) return true
    if ((pa[i] ?? 0) < (pb[i] ?? 0)) return false
  }
  return false
}

interface OnboardingState {
  mode: 'none' | 'first-launch' | 'upgrade'
  currentStep: number
  steps: OnboardingStep[]
  completedIds: SvelteSet<string>
  selectedTools: SvelteSet<string>
  fromVersion?: string
}

export const onboardingState: OnboardingState = $state({
  mode: 'none',
  currentStep: 0,
  steps: [],
  completedIds: new SvelteSet(),
  selectedTools: new SvelteSet(['claude']),
})

export async function initOnboarding(
  mode: 'first-launch' | 'upgrade',
  fromVersion?: string,
): Promise<void> {
  const completed = await window.api.getOnboardingCompleted()
  const completedIds = new SvelteSet(completed)

  let steps: OnboardingStep[]
  if (mode === 'first-launch') {
    steps = getFirstLaunchSteps()
  } else {
    steps = getFeatureSteps().filter(
      (s) => !completedIds.has(s.id) && fromVersion && semverGt(s.introducedIn, fromVersion),
    )
  }

  if (steps.length === 0) {
    onboardingState.mode = 'none'
    return
  }

  onboardingState.mode = mode
  onboardingState.currentStep = 0
  onboardingState.steps = steps
  onboardingState.completedIds = completedIds
  onboardingState.fromVersion = fromVersion
}

export function currentStepDef(): OnboardingStep | undefined {
  return onboardingState.steps[onboardingState.currentStep]
}

export async function completeCurrentStep(): Promise<void> {
  const step = currentStepDef()
  if (!step) return
  onboardingState.completedIds.add(step.id)
}

export async function nextStep(): Promise<boolean> {
  await completeCurrentStep()
  if (onboardingState.currentStep < onboardingState.steps.length - 1) {
    onboardingState.currentStep++
    return true
  }
  return false
}

export function prevStep(): void {
  if (onboardingState.currentStep > 0) {
    onboardingState.currentStep--
  }
}

export async function finishOnboarding(): Promise<void> {
  const aboutInfo = await window.api.getAboutInfo()
  const allStepIds = onboardingState.steps.map((s) => s.id)
  await window.api.completeOnboarding(allStepIds, aboutInfo.version)
  onboardingState.mode = 'none'
  onboardingState.steps = []
  onboardingState.currentStep = 0
  onboardingState.selectedTools = new SvelteSet(['claude'])
}

export async function skipOnboarding(): Promise<void> {
  await finishOnboarding()
}
