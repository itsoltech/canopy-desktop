import { execFile } from 'child_process'
import os from 'os'
import type { Database as BetterSqlite3Database } from 'better-sqlite3'
import type { Database } from '../db/Database'
import type { ToolDefinition, ToolDefinitionRow } from '../db/types'
import { toolFromRow } from '../db/types'

export class ToolRegistry {
  private tools = new Map<string, ToolDefinition>()

  constructor(private database: Database) {
    this.reload()
  }

  private get db(): BetterSqlite3Database {
    return this.database.db
  }

  private reload(): void {
    this.tools.clear()
    const rows = this.db.prepare('SELECT * FROM tool_definitions').all() as ToolDefinitionRow[]
    for (const row of rows) {
      this.tools.set(row.id, toolFromRow(row))
    }
  }

  getAll(): ToolDefinition[] {
    return Array.from(this.tools.values())
  }

  get(id: string): ToolDefinition | undefined {
    return this.tools.get(id)
  }

  getByCategory(category: string): ToolDefinition[] {
    return this.getAll().filter((t) => t.category === category)
  }

  addCustom(tool: {
    id: string
    name: string
    command: string
    args?: string[]
    icon?: string
    category?: string
  }): void {
    this.db
      .prepare(
        `INSERT INTO tool_definitions (id, name, command, args_json, icon, category, is_custom)
         VALUES (?, ?, ?, ?, ?, ?, 1)`
      )
      .run(
        tool.id,
        tool.name,
        tool.command,
        JSON.stringify(tool.args ?? []),
        tool.icon ?? 'terminal',
        tool.category ?? 'system'
      )
    this.reload()
  }

  removeCustom(id: string): void {
    this.db.prepare('DELETE FROM tool_definitions WHERE id = ? AND is_custom = 1').run(id)
    this.reload()
  }

  resolveCommand(tool: ToolDefinition): string {
    if (tool.id === 'shell' || tool.command === 'shell') {
      if (os.platform() === 'win32') {
        return 'powershell.exe'
      }
      return process.env.SHELL || '/bin/bash'
    }
    return tool.command
  }

  async checkAvailability(): Promise<Record<string, boolean>> {
    const result: Record<string, boolean> = {}
    const cmd = os.platform() === 'win32' ? 'where' : 'which'

    const checks = this.getAll().map(
      (tool) =>
        new Promise<void>((resolve) => {
          const binary = this.resolveCommand(tool)
          execFile(cmd, [binary], (err) => {
            result[tool.id] = !err
            resolve()
          })
        })
    )

    await Promise.all(checks)
    return result
  }
}
