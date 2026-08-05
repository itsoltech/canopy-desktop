import { describe, expect, it } from 'vitest'
import {
  isEncryptedPreferenceKey,
  isExportablePreferenceKey,
  isMainProcessOnlyPreferenceKey,
} from './preferenceKeys'

describe('preference key policy', () => {
  it.each([
    'credential.registry.v2',
    'credential.bindings.v2',
    'credential.secret.v2.credential-id',
    'taskTracker.token.jira:https://jira.example.com',
  ])('keeps %s behind main-process-only preference IPC', (key) => {
    expect(isMainProcessOnlyPreferenceKey(key)).toBe(true)
  })

  it('keeps registry metadata non-exportable without treating it as encrypted secret text', () => {
    expect(isEncryptedPreferenceKey('credential.registry.v2')).toBe(false)
    expect(isExportablePreferenceKey('credential.registry.v2')).toBe(false)
    expect(isMainProcessOnlyPreferenceKey('credential.registry.v2')).toBe(true)
  })

  it('allows ordinary renderer preferences through the same policy', () => {
    expect(isMainProcessOnlyPreferenceKey('sidebar.sections')).toBe(false)
    expect(isExportablePreferenceKey('sidebar.sections')).toBe(true)
  })
})
