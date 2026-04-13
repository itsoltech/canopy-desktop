import { app } from 'electron'
import os from 'os'
import { existsSync, readFileSync, statSync, unlinkSync, writeFileSync } from 'fs'
import { join } from 'path'
import { findRecentNativeCrash, type NativeCrashInfo } from './NativeCrashReader'

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
  nativeCrash?: {
    exceptionType?: string
    exceptionCodes?: string
    terminationReason?: string
    triggeredThread?: string
    incidentId?: string
    sourceFile?: string
  }
}

const NATIVE_CRASH_PROCESS_NAME = 'Canopy'

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
        errorMessage: error.message,
        stack: error.stack,
        appVersion: app.getVersion(),
        electronVersion: process.versions.electron,
        os: `${os.platform()} ${os.release()} ${os.arch()}`,
      })
    } catch {
      // Crash reporter must never throw
    }
  }

  getCrashReport(): CrashReport | null {
    try {
      if (!existsSync(this.reportPath)) return null
      const raw = readFileSync(this.reportPath, 'utf-8')
      return JSON.parse(raw) as CrashReport
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
    }

    if (!native) {
      return {
        ...base,
        timestamp: new Date().toISOString(),
        errorMessage: 'The app did not shut down cleanly',
      }
    }

    const where = native.triggeredThread ? ` in ${native.triggeredThread}` : ''
    const what = native.exceptionType ?? 'unknown exception'
    return {
      ...base,
      timestamp: native.timestamp || new Date().toISOString(),
      errorMessage: `Native crash: ${what}${where}`,
      stack: native.stack,
      nativeCrash: {
        exceptionType: native.exceptionType,
        exceptionCodes: native.exceptionCodes,
        terminationReason: native.terminationReason,
        triggeredThread: native.triggeredThread,
        incidentId: native.incidentId,
        sourceFile: native.sourceFile,
      },
    }
  }

  private writeCrashReport(report: CrashReport): void {
    writeFileSync(this.reportPath, JSON.stringify(report, null, 2))
  }
}
