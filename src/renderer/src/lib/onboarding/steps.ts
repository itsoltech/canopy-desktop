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
    id: 'tool-selection',
    title: 'Choose your tools',
    description: 'Select the AI assistants you plan to use with Canopy.',
    introducedIn: '0.9.0',
    category: 'first-launch',
    order: 1,
  },
  {
    id: 'environment-check',
    title: 'Environment check',
    description: 'Verify that selected tools are installed and accessible.',
    introducedIn: '0.9.0',
    category: 'first-launch',
    order: 2,
  },
  {
    id: 'theme',
    title: 'Choose your theme',
    description: 'Pick a terminal color scheme that fits your style.',
    introducedIn: '0.9.0',
    category: 'first-launch',
    order: 3,
  },
  {
    id: 'ai-setup',
    title: 'AI assistant',
    description: 'Configure Claude Code to work from within Canopy.',
    introducedIn: '0.9.0',
    category: 'first-launch',
    order: 4,
  },
  {
    id: 'features',
    title: 'Customize features',
    description: 'Toggle features to match how you work.',
    introducedIn: '0.9.0',
    category: 'first-launch',
    order: 5,
  },
  {
    id: 'ready',
    title: "You're all set",
    description: 'Open a project folder to get started.',
    introducedIn: '0.9.0',
    category: 'first-launch',
    order: 6,
  },
  // Feature onboarding steps (shown on upgrade)
  {
    id: 'task-tracker',
    title: 'Connect your task tracker',
    description:
      'Link Jira or YouTrack to browse tasks, create branches, and open PRs — all from Canopy.',
    introducedIn: '0.9.0',
    category: 'feature',
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
