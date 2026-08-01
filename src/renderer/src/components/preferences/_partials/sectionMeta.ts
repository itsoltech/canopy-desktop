import {
  Settings2,
  RefreshCw,
  Shield,
  Keyboard,
  Bell,
  Ellipsis,
  Palette,
  PanelLeft,
  Sparkles,
  Gem,
  Code,
  Braces,
  Wrench,
  SquareTerminal,
  Hammer,
  GitBranch,
  KeyRound,
  FolderSearch,
  Globe,
  Smartphone,
  Workflow,
} from '@lucide/svelte'
import type { Component } from 'svelte'

export interface SectionMeta {
  icon: Component
  description: string
  keywords: string
}

export const sectionMeta: Record<string, SectionMeta> = {
  General: {
    icon: Settings2,
    description: 'Startup behavior, default tools, and status bar',
    keywords: 'reopen workspace startup new tab worktree shell cpu ram performance hud',
  },
  Updates: {
    icon: RefreshCw,
    description: 'Auto-update channel and check frequency',
    keywords: 'auto update channel stable next hourly daily weekly version',
  },
  Privacy: {
    icon: Shield,
    description: 'Telemetry and diagnostics',
    keywords: 'telemetry diagnostics privacy analytics ping',
  },
  Shortcuts: {
    icon: Keyboard,
    description: 'Customize keyboard shortcuts',
    keywords: 'shortcuts hotkeys keybindings keyboard',
  },
  Notch: {
    icon: Bell,
    description: 'macOS notch overlay for session status',
    keywords: 'notch overlay status indicator macos session',
  },
  Misc: {
    icon: Ellipsis,
    description: 'Other toggles and tweaks',
    keywords: 'misc miscellaneous other',
  },
  Appearance: {
    icon: Palette,
    description: 'Terminal theme and font',
    keywords: 'theme color font family size appearance terminal jetbrains mono',
  },
  Sidebar: {
    icon: PanelLeft,
    description: 'Sidebar layout and visibility',
    keywords: 'sidebar layout visibility panel',
  },
  Claude: {
    icon: Sparkles,
    description: 'Claude Code integration',
    keywords: 'claude anthropic api key model permission effort plan auto bypass bedrock vertex',
  },
  Gemini: {
    icon: Gem,
    description: 'Gemini integration',
    keywords: 'gemini google api key model approval',
  },
  OpenCode: {
    icon: Code,
    description: 'OpenCode integration',
    keywords: 'opencode api key model',
  },
  Codex: {
    icon: Braces,
    description: 'Codex integration',
    keywords: 'codex openai approval sandbox full auto profile',
  },
  Skills: {
    icon: Wrench,
    description: 'Available skills',
    keywords: 'skills tools agents',
  },
  Terminal: {
    icon: SquareTerminal,
    description: 'tmux session persistence and terminal behavior',
    keywords: 'terminal tmux mouse close detach kill ask session persistence',
  },
  Tools: {
    icon: Hammer,
    description: 'Custom tools and commands',
    keywords: 'tools custom command icon category shell git ai',
  },
  Git: {
    icon: GitBranch,
    description: 'Pull strategy and worktree setup',
    keywords: 'git pull rebase merge worktree directory base setup actions copy command',
  },
  'Your connections': {
    icon: KeyRound,
    description: 'Your personal tracker connections and locally-stored credentials',
    keywords:
      'your personal connections tracker jira linear youtrack github token credentials auth baseurl',
  },
  'CI/CD': {
    icon: Workflow,
    description: "Build status and triggering for this project's TeamCity server",
    keywords: 'ci cd teamcity build deploy pipeline status trigger token configuration',
  },
  'File Watcher': {
    icon: FolderSearch,
    description: 'Ignore patterns for the workspace file watcher',
    keywords: 'file watcher ignore patterns gitignore exclude',
  },
  'Web Browser': {
    icon: Globe,
    description: 'Webview viewports and devices',
    keywords: 'web browser viewport device webview responsive',
  },
  'Remote Control': {
    icon: Smartphone,
    description:
      'Control Canopy from the Canopy Remote mobile app on your local network or via VPN. The mobile app will be available soon.',
    keywords: 'remote control phone session trusted device port pair',
  },
}

export function sectionMatches(name: string, query: string): boolean {
  const q = query.trim().toLowerCase()
  if (!q) return true
  const meta = sectionMeta[name]
  const haystack = `${name} ${meta?.description ?? ''} ${meta?.keywords ?? ''}`.toLowerCase()
  return haystack.includes(q)
}
