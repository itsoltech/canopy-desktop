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
  {
    id: 'opencode',
    title: 'OpenCode integration',
    description:
      'OpenCode is now available as an AI agent in Canopy. Launch it from the tool launcher and configure it in Settings → OpenCode.',
    introducedIn: '0.11.0',
    category: 'feature',
  },
  {
    id: 'perf-hud',
    title: 'CPU and RAM in the status bar',
    description:
      'Enable "Show CPU and RAM usage in status bar" in Settings → General to see total CPU and memory across all Canopy processes, sampled once per second. Off by default — the sampler only runs while the indicator is visible, so there is no overhead when it is disabled.',
    introducedIn: '0.11.0',
    category: 'feature',
  },
  {
    id: 'pane-drag',
    title: 'Drag panes to rearrange splits',
    description:
      'Hold Alt (Option on Mac) and drag any pane to reorder it within a split, move it to another tab, or drop it on the tab bar to detach it into its own tab.',
    introducedIn: '0.11.0',
    category: 'feature',
  },
  {
    id: 'agent-profiles',
    title: 'Multiple profiles per AI agent',
    description:
      'Each AI agent (Claude, Gemini, OpenCode, Codex) now supports named profiles. Point one profile at Anthropic, another at Ollama, another at GLM or MinMax — each with its own API key, base URL, model, and env vars. Create profiles in Settings → (agent name), then expand the agent in the Tools sidebar to launch a specific profile. Your existing settings were migrated into a "Default" profile automatically.',
    introducedIn: '0.11.0',
    category: 'feature',
  },
  {
    id: 'status-bar-settings',
    title: 'Quick Settings access',
    description:
      'The gear icon at the right of the status bar opens Settings directly. You can also use ⌘, / Ctrl+, or the command palette.',
    introducedIn: '0.12.0',
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
