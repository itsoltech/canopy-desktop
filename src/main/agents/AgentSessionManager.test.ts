import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterAll, describe, expect, it, vi } from 'vitest'

const userData = mkdtempSync(join(tmpdir(), 'canopy-agents-'))

vi.mock('electron', () => ({
  app: { getPath: () => userData, isPackaged: false },
  BrowserWindow: { getAllWindows: () => [] },
  Notification: class {},
}))
vi.mock('@electron-toolkit/utils', () => ({ is: { dev: false } }))

const { AgentSessionManager } = await import('./AgentSessionManager')

describe('AgentSessionManager.getResumeArgs', () => {
  const manager = new AgentSessionManager()

  afterAll(() => {
    rmSync(userData, { recursive: true, force: true })
  })

  it('builds resume args for a well-formed session id', () => {
    const id = '2f1d6c3e-8b7a-4c2d-9e1f-0a1b2c3d4e5f'
    expect(manager.getResumeArgs('claude', id)).toEqual(['--resume', id])
    expect(manager.getResumeArgs('codex', id)).toEqual(['resume', id])
  })

  it('drops a persisted session id that would be parsed as a CLI option', () => {
    expect(manager.getResumeArgs('claude', '--dangerously-skip-permissions')).toEqual([])
    expect(manager.getResumeArgs('codex', '--dangerously-bypass-approvals-and-sandbox')).toEqual([])
  })
})
