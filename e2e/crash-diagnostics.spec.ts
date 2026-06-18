import { test, expect } from '@playwright/test'
import { matchesNativeCrashProcessName } from '../src/main/crash/NativeCrashReader'
import { sanitizeDiagnosticText } from '../src/main/crash/sanitizeCrashDiagnostic'
import { formatCrashReportMarkdown } from '../src/renderer/src/lib/crashReportMarkdown'

test('crash diagnostics keep useful stack symbols while redacting common secret formats', () => {
  const sanitized = sanitizeDiagnosticText(`
    at tokenize (/Users/alice/project/tokenize.ts:12:3)
    at secretStore (/Users/alice/project/secretStore.ts:5:7)
    Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ==
    eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.sflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c
    AKIAIOSFODNN7EXAMPLE
    ghp_abcdefghijklmnopqrstuvwxyz123456
    -----BEGIN PRIVATE KEY-----
    secret-key-material
    -----END PRIVATE KEY-----
    session=abc123secret
  `)

  expect(sanitized).toContain('tokenize')
  expect(sanitized).toContain('secretStore')
  expect(sanitized).not.toContain('QWxhZGRpbjpvcGVuIHNlc2FtZQ==')
  expect(sanitized).not.toContain('eyJhbGciOiJIUzI1NiJ9')
  expect(sanitized).not.toContain('AKIAIOSFODNN7EXAMPLE')
  expect(sanitized).not.toContain('ghp_abcdefghijklmnopqrstuvwxyz123456')
  expect(sanitized).not.toContain('secret-key-material')
  expect(sanitized).not.toContain('abc123secret')
})

test('renderer native crash matching accepts Electron helper process variants', () => {
  expect(matchesNativeCrashProcessName('Canopy Helper (Renderer)', 'Canopy Helper')).toBe(true)
  expect(
    matchesNativeCrashProcessName('Canopy Helper (Renderer)', 'Canopy Helper (Renderer)'),
  ).toBe(true)
  expect(matchesNativeCrashProcessName('Canopy', 'Canopy Helper (Renderer)')).toBe(false)
})

test('crash report markdown renders renderer and native crash details with safe fences', () => {
  const markdown = formatCrashReportMarkdown({
    timestamp: '2026-06-18T06:00:00.000Z',
    type: 'rendererCrash',
    process: 'renderer',
    errorMessage: 'Renderer crashed with ``` in message',
    appVersion: '0.13.0',
    electronVersion: '41.0.0',
    os: 'darwin 25 arm64',
    renderer: {
      reason: 'crashed',
      exitCode: 139,
    },
    nativeCrash: {
      exceptionType: 'EXC_BAD_ACCESS',
      triggeredThread: 'CrRendererMain',
      stack: '0 Canopy Helper tokenize + 4',
    },
  })

  expect(markdown).toContain('- **Process:** renderer')
  expect(markdown).toContain('- **Renderer reason:** crashed')
  expect(markdown).toContain('- **Renderer exit code:** 139')
  expect(markdown).toContain('### Native crash')
  expect(markdown).toContain('#### Native stack')
  expect(markdown).toContain('````\nRenderer crashed with ``` in message\n````')
})
