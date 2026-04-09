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
  {
    id: 'telemetry',
    title: 'Minimal telemetry',
    description:
      'Canopy sends one daily ping so we know how many people use it. The ping contains screen resolution, locale, app version, and OS — nothing else. You can disable this in Settings → Privacy.',
    introducedIn: '0.10.0',
    category: 'feature',
  },
  {
    id: 'run-configurations',
    title: 'Run Configurations',
    description:
      'Define project commands in .canopy/run.toml and launch them from the sidebar or titlebar. Supports monorepos, environment variables, pre/post hooks, and instance limits. Enable the RUN section in Sidebar preferences.',
    introducedIn: '0.11.0',
    category: 'feature',
  },
  {
    id: 'worktree-existing-branch',
    title: 'Create worktrees from existing branches',
    description:
      'The Create Worktree dialog now has a "From existing branch" mode — pick any local or remote branch and check it out into a new worktree in one step. Remote-only branches are created as local tracking branches automatically.',
    introducedIn: '0.10.0',
    category: 'feature',
  },
  {
    id: 'remote-control',
    title: 'Remote control (Beta)',
    description:
      'Mirror and control this Canopy window from your phone, tablet, or another laptop on the same WiFi. Enable it in Settings → Remote Control, then open "Remote Connection" from the command palette to scan a QR code and pair a device. Beta — expect rough edges while we iterate.',
    introducedIn: '0.10.0',
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
