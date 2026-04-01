import type { Database as BetterSqlite3Database } from 'better-sqlite3'
import type { Database } from './Database'

export class OnboardingStore {
  constructor(private database: Database) {}

  private get db(): BetterSqlite3Database {
    return this.database.db
  }

  getCompleted(): string[] {
    const rows = this.db.prepare('SELECT step_id FROM onboarding_completions').all() as {
      step_id: string
    }[]
    return rows.map((r) => r.step_id)
  }

  isCompleted(stepId: string): boolean {
    const row = this.db
      .prepare('SELECT 1 FROM onboarding_completions WHERE step_id = ?')
      .get(stepId)
    return row !== undefined
  }

  complete(stepId: string, appVersion: string): void {
    this.db
      .prepare(`INSERT OR IGNORE INTO onboarding_completions (step_id, app_version) VALUES (?, ?)`)
      .run(stepId, appVersion)
  }

  completeMany(stepIds: string[], appVersion: string): void {
    const insert = this.db.prepare(
      `INSERT OR IGNORE INTO onboarding_completions (step_id, app_version) VALUES (?, ?)`,
    )
    const batch = this.db.transaction((ids: string[]) => {
      for (const id of ids) {
        insert.run(id, appVersion)
      }
    })
    batch(stepIds)
  }

  reset(): void {
    this.db.exec('DELETE FROM onboarding_completions')
  }
}
