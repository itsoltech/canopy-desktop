import type { CrashReportData } from '../../../renderer-shared/crashReport'

export function formatCrashReportMarkdown(d: CrashReportData): string {
  const lines = [
    '## Crash report',
    '',
    `- **Timestamp:** ${d.timestamp}`,
    `- **Type:** ${d.type}`,
    `- **Process:** ${d.process ?? 'unknown'}`,
    `- **App version:** ${d.appVersion}`,
    `- **Electron:** ${d.electronVersion}`,
    `- **OS:** ${d.os}`,
  ]

  if (d.renderer?.reason) lines.push(`- **Renderer reason:** ${d.renderer.reason}`)
  if (d.renderer?.exitCode !== undefined)
    lines.push(`- **Renderer exit code:** ${d.renderer.exitCode}`)

  appendFencedSection(lines, '### Error', d.errorMessage)

  if (d.stack) {
    appendFencedSection(lines, '### Stack trace', d.stack)
  }

  if (d.nativeCrash) {
    lines.push('', '### Native crash')
    if (d.nativeCrash.exceptionType) {
      lines.push(`- **Exception:** ${d.nativeCrash.exceptionType}`)
    }
    if (d.nativeCrash.exceptionCodes) {
      lines.push(`- **Exception codes:** ${d.nativeCrash.exceptionCodes}`)
    }
    if (d.nativeCrash.terminationReason) {
      lines.push(`- **Termination:** ${d.nativeCrash.terminationReason}`)
    }
    if (d.nativeCrash.triggeredThread) {
      lines.push(`- **Triggered thread:** ${d.nativeCrash.triggeredThread}`)
    }
    if (d.nativeCrash.incidentId) {
      lines.push(`- **Incident ID:** ${d.nativeCrash.incidentId}`)
    }
    if (d.nativeCrash.stack) {
      appendFencedSection(lines, '#### Native stack', d.nativeCrash.stack)
    }
  }

  return lines.join('\n')
}

function appendFencedSection(lines: string[], heading: string, value: string): void {
  const fence = markdownFenceFor(value)
  lines.push('', heading, fence, value, fence)
}

function markdownFenceFor(value: string): string {
  const backtickRuns = value.match(/`+/g) ?? []
  const longestRun = backtickRuns.reduce((max, run) => Math.max(max, run.length), 0)
  return '`'.repeat(Math.max(3, longestRun + 1))
}
