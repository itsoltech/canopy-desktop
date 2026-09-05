import { describe, expect, it, vi } from 'vitest'

vi.mock('./preferences.svelte', () => ({
  setPref: vi.fn(),
}))

import {
  getSidebarConfig,
  setSidebarSectionVisibility,
  type SidebarSectionConfig,
} from './sidebarSections.svelte'

function configWithGitModes(
  gitVisible: boolean,
  pullRequestsVisible: boolean,
): SidebarSectionConfig[] {
  return [
    { id: 'projects', visible: true },
    { id: 'git', visible: gitVisible },
    { id: 'pullRequests', visible: pullRequestsVisible },
  ]
}

describe('sidebar Git display modes', () => {
  it('adds Pull requests only after Git when migrating saved preferences', () => {
    const config = getSidebarConfig(
      JSON.stringify([
        { id: 'projects', visible: true },
        { id: 'git', visible: true },
        { id: 'tools', visible: true },
      ]),
    )

    const gitIndex = config.findIndex((item) => item.id === 'git')
    expect(config[gitIndex + 1]).toEqual({ id: 'pullRequests', visible: false })
  })

  it('prefers the full Git section when malformed preferences enable both modes', () => {
    const config = getSidebarConfig(JSON.stringify(configWithGitModes(true, true)))

    expect(config.find((item) => item.id === 'git')?.visible).toBe(true)
    expect(config.find((item) => item.id === 'pullRequests')?.visible).toBe(false)
  })

  it('enables Pull requests only and disables the full Git section', () => {
    const config = setSidebarSectionVisibility(
      configWithGitModes(true, false),
      'pullRequests',
      true,
    )

    expect(config.find((item) => item.id === 'git')?.visible).toBe(false)
    expect(config.find((item) => item.id === 'pullRequests')?.visible).toBe(true)
  })

  it('enables the full Git section and disables Pull requests only', () => {
    const config = setSidebarSectionVisibility(configWithGitModes(false, true), 'git', true)

    expect(config.find((item) => item.id === 'git')?.visible).toBe(true)
    expect(config.find((item) => item.id === 'pullRequests')?.visible).toBe(false)
  })

  it('allows hiding the active Git mode', () => {
    const config = setSidebarSectionVisibility(
      configWithGitModes(false, true),
      'pullRequests',
      false,
    )

    expect(config.find((item) => item.id === 'git')?.visible).toBe(false)
    expect(config.find((item) => item.id === 'pullRequests')?.visible).toBe(false)
  })
})
