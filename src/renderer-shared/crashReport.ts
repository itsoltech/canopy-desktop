export type CrashReportType =
  | 'uncaughtException'
  | 'unhandledRejection'
  | 'rendererCrash'
  | 'childProcessGone'
  | 'ungracefulShutdown'

export type CrashReportProcess = 'main' | 'renderer' | 'child' | 'unknown'

export interface CrashReportData {
  timestamp: string
  type: CrashReportType
  errorMessage: string
  stack?: string
  appVersion: string
  electronVersion: string
  os: string
  process?: CrashReportProcess
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

export type CrashReport = CrashReportData
