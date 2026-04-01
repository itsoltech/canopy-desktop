export interface OnboardingStep {
  id: string
  title: string
  description: string
  introducedIn: string
  category: 'first-launch' | 'feature'
  order?: number
}

export const onboardingSteps: OnboardingStep[] = [
  // First-launch wizard steps
  {
    id: 'welcome',
    title: 'Welcome to Canopy',
    description: 'A developer terminal workstation built for parallel workflows.',
    introducedIn: '0.9.0',
    category: 'first-launch',
    order: 0,
  },
  {
    id: 'theme',
    title: 'Choose your theme',
    description: 'Pick a terminal color scheme that fits your style.',
    introducedIn: '0.9.0',
    category: 'first-launch',
    order: 1,
  },
  {
    id: 'ai-setup',
    title: 'AI assistant',
    description: 'Configure Claude Code to work from within Canopy.',
    introducedIn: '0.9.0',
    category: 'first-launch',
    order: 2,
  },
  {
    id: 'features',
    title: 'Customize features',
    description: 'Toggle features to match how you work.',
    introducedIn: '0.9.0',
    category: 'first-launch',
    order: 3,
  },
  {
    id: 'ready',
    title: "You're all set",
    description: 'Open a project folder to get started.',
    introducedIn: '0.9.0',
    category: 'first-launch',
    order: 4,
  },
]

export function getFirstLaunchSteps(): OnboardingStep[] {
  return onboardingSteps
    .filter((s) => s.category === 'first-launch')
    .sort((a, b) => (a.order ?? 0) - (b.order ?? 0))
}

export function getFeatureSteps(): OnboardingStep[] {
  return onboardingSteps.filter((s) => s.category === 'feature')
}
