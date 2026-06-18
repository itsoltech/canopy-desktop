import { app, type RenderProcessGoneDetails } from 'electron'
import os from 'os'
import { existsSync, readFileSync, statSync, unlinkSync, writeFileSync } from 'fs'
import { join } from 'path'
import { match, P } from 'ts-pattern'
import { findRecentNativeCrash, type NativeCrashInfo } from './NativeCrashReader'
import { sanitizeDiagnosticText, sanitizeStack } from './sanitizeCrashDiagnostic'

export interface CrashReport {
  timestamp: string
  type:
    | 'uncaughtException'
    | 'unhandledRejection'
    | 'rendererCrash'
    | 'childProcessGone'
    | 'ungracefulShutdown'
  errorMessage: string
  stack?: string
  appVersion: string
  electronVersion: string
  os: string
  process?: 'main' | 'renderer' | 'child' | 'unknown'
  renderer?: {
    reason?: string
    exitCode?: number
  }
  nativeCrash?: {
    exceptionType?: string
    exceptionCodes?: string
    terminationReason?: string
    triggeredThread?: string
    incidentId?: string
    stack?: string
  }
}

const NATIVE_CRASH_PROCESS_NAME = 'Canopy'
const RENDERER_CRASH_PROCESS_NAME = 'Canopy Helper (Renderer)'

export class CrashReporter {
  private readonly sentinelPath: string
  private readonly reportPath: string

  constructor() {
    const dir = app.getPath('userData')
    this.sentinelPath = join(dir, '.canopy-running')
    this.reportPath = join(dir, 'crash-report.json')
  }

  init(): void {
    try {
      if (existsSync(this.sentinelPath) && !existsSync(this.reportPath)) {
        const prevSentinelMs = this.readSentinelMtime()
        const native = findRecentNativeCrash(NATIVE_CRASH_PROCESS_NAME, prevSentinelMs)
        this.writeCrashReport(this.buildUngracefulShutdownReport(native))
      }
      writeFileSync(this.sentinelPath, String(process.pid))
    } catch {
      // Crash reporter must never throw
    }
  }

  recordCrash(type: CrashReport['type'], error: Error): void {
    try {
      this.writeCrashReport({
        timestamp: new Date().toISOString(),
        type,
        errorMessage: sanitizeRequired(error.message),
        stack: sanitizeStack(error.stack),
        appVersion: app.getVersion(),
        electronVersion: process.versions.electron,
        os: `${os.platform()} ${os.release()} ${os.arch()}`,
        process: this.processForType(type),
      })
    } catch {
      // Crash reporter must never throw
    }
  }

  recordRendererCrash(details: RenderProcessGoneDetails): void {
    try {
      const timestamp = new Date().toISOString()
      const crashMs = Date.parse(timestamp)
      const native = findRecentNativeCrash(RENDERER_CRASH_PROCESS_NAME, crashMs)
      this.writeCrashReport({
        timestamp,
        type: 'rendererCrash',
        errorMessage: sanitizeRequired(`Renderer crashed: ${details.reason}`),
        stack: sanitizeStack(new Error(`Renderer crashed: ${details.reason}`).stack),
        appVersion: app.getVersion(),
        electronVersion: process.versions.electron,
        os: `${os.platform()} ${os.release()} ${os.arch()}`,
        process: 'renderer',
        renderer: {
          reason: sanitizeDiagnosticText(details.reason),
          exitCode: details.exitCode,
        },
        nativeCrash: this.toPublicNativeCrash(native),
      })
    } catch {
      // Crash reporter must never throw
    }
  }

  getCrashReport(): CrashReport | null {
    try {
      if (!existsSync(this.reportPath)) return null
      const raw = readFileSync(this.reportPath, 'utf-8')
      const report = JSON.parse(raw) as CrashReport
      return this.toPublicCrashReport(this.enrichRendererCrashReport(report))
    } catch {
      return null
    }
  }

  clearCrashReport(): void {
    try {
      if (existsSync(this.reportPath)) unlinkSync(this.reportPath)
    } catch {
      // Crash reporter must never throw
    }
  }

  clearSentinel(): void {
    try {
      if (existsSync(this.sentinelPath)) unlinkSync(this.sentinelPath)
    } catch {
      // Crash reporter must never throw
    }
  }

  private readSentinelMtime(): number {
    try {
      return statSync(this.sentinelPath).mtimeMs
    } catch {
      return 0
    }
  }

  private buildUngracefulShutdownReport(native: NativeCrashInfo | null): CrashReport {
    const base = {
      type: 'ungracefulShutdown' as const,
      appVersion: app.getVersion(),
      electronVersion: process.versions.electron,
      os: `${os.platform()} ${os.release()} ${os.arch()}`,
      process: 'main' as const,
    }

    if (!native) {
      return {
        ...base,
        timestamp: new Date().toISOString(),
        errorMessage: sanitizeRequired('The app did not shut down cleanly'),
      }
    }

    const where = native.triggeredThread ? ` in ${native.triggeredThread}` : ''
    const what = native.exceptionType ?? 'unknown exception'
    return {
      ...base,
      timestamp: native.timestamp || new Date().toISOString(),
      errorMessage: sanitizeRequired(`Native crash: ${what}${where}`),
      stack: sanitizeStack(native.stack),
      nativeCrash: this.toPublicNativeCrash(native),
    }
  }

  private enrichRendererCrashReport(report: CrashReport): CrashReport {
    if (report.type !== 'rendererCrash' || report.nativeCrash?.stack) return report

    const crashMs = Date.parse(report.timestamp)
    if (Number.isNaN(crashMs)) return report

    const native = findRecentNativeCrash(RENDERER_CRASH_PROCESS_NAME, crashMs)
    const nativeCrash = this.toPublicNativeCrash(native)
    if (!nativeCrash) return report

    return {
      ...report,
      process: 'renderer',
      nativeCrash,
    }
  }

  private toPublicCrashReport(report: CrashReport): CrashReport {
    return {
      timestamp: sanitizeRequired(report.timestamp),
      type: report.type,
      errorMessage: sanitizeRequired(report.errorMessage),
      stack: sanitizeStack(report.stack),
      appVersion: sanitizeRequired(report.appVersion),
      electronVersion: sanitizeRequired(report.electronVersion),
      os: sanitizeRequired(report.os),
      process: report.process ?? this.processForType(report.type),
      renderer: report.renderer
        ? {
            reason: sanitizeDiagnosticText(report.renderer.reason),
            exitCode: report.renderer.exitCode,
          }
        : undefined,
      nativeCrash: report.nativeCrash
        ? {
            exceptionType: sanitizeDiagnosticText(report.nativeCrash.exceptionType),
            exceptionCodes: sanitizeDiagnosticText(report.nativeCrash.exceptionCodes),
            terminationReason: sanitizeDiagnosticText(report.nativeCrash.terminationReason),
            triggeredThread: sanitizeDiagnosticText(report.nativeCrash.triggeredThread),
            incidentId: sanitizeDiagnosticText(report.nativeCrash.incidentId),
            stack: sanitizeStack(report.nativeCrash.stack),
          }
        : undefined,
    }
  }

  private toPublicNativeCrash(native: NativeCrashInfo | null): CrashReport['nativeCrash'] {
    if (!native) return undefined
    return {
      exceptionType: sanitizeDiagnosticText(native.exceptionType),
      exceptionCodes: sanitizeDiagnosticText(native.exceptionCodes),
      terminationReason: sanitizeDiagnosticText(native.terminationReason),
      triggeredThread: sanitizeDiagnosticText(native.triggeredThread),
      incidentId: sanitizeDiagnosticText(native.incidentId),
      stack: sanitizeStack(native.stack),
    }
  }

  private processForType(type: CrashReport['type']): NonNullable<CrashReport['process']> {
    return match(type)
      .with('rendererCrash', () => 'renderer' as const)
      .with('childProcessGone', () => 'child' as const)
      .with(
        P.union('uncaughtException', 'unhandledRejection', 'ungracefulShutdown'),
        () => 'main' as const,
      )
      .exhaustive()
  }

  private writeCrashReport(report: CrashReport): void {
    writeFileSync(this.reportPath, JSON.stringify(this.toPublicCrashReport(report), null, 2))
  }
}

function sanitizeRequired(value: string): string {
  return sanitizeDiagnosticText(value) ?? ''
}
