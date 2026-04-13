import { app } from 'electron'
import os from 'os'
import { existsSync, readFileSync, unlinkSync, writeFileSync } from 'fs'
import { join } from 'path'

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
}

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
        this.writeCrashReport({
          timestamp: new Date().toISOString(),
          type: 'ungracefulShutdown',
          errorMessage: 'The app did not shut down cleanly',
          appVersion: app.getVersion(),
          electronVersion: process.versions.electron,
          os: `${os.platform()} ${os.release()} ${os.arch()}`,
        })
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

  private writeCrashReport(report: CrashReport): void {
    writeFileSync(this.reportPath, JSON.stringify(report, null, 2))
  }
}
