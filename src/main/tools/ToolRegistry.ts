import { execFile } from 'child_process'
import os from 'os'
import type { Database as BetterSqlite3Database } from 'better-sqlite3'
import type { Database } from '../db/Database'
import type { ToolDefinition, ToolDefinitionRow } from '../db/types'
import { toolFromRow } from '../db/types'
import { getLoginEnv } from '../shell/loginEnv'

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
    if (!tool.command.trim()) {
      throw new Error('Command cannot be empty')
    }
    // Block shell metacharacters for both Unix and Windows (cmd.exe: % ^ !)
    if (/[/\\;|&$`<>%^!()"]/.test(tool.command)) {
      throw new Error(
        'Invalid command: must be a simple binary name without path separators or shell metacharacters',
      )
    }
    // Validate args — block shell metacharacters (Unix + cmd.exe /c context)
    const SHELL_META = /[;|&$`<>%^!()\\"]/
    if (tool.args?.some((arg) => SHELL_META.test(arg))) {
      throw new Error('Invalid args: contain shell metacharacters')
    }
    this.db
      .prepare(
        `INSERT INTO tool_definitions (id, name, command, args_json, icon, category, is_custom)
         VALUES (?, ?, ?, ?, ?, ?, 1)`,
      )
      .run(
        tool.id,
        tool.name,
        tool.command,
        JSON.stringify(tool.args ?? []),
        tool.icon ?? 'terminal',
        tool.category ?? 'system',
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
      const env = getLoginEnv()
      return env?.SHELL || process.env.SHELL || '/bin/bash'
    }
    return tool.command
  }

  async checkAvailability(): Promise<Record<string, boolean>> {
    const result: Record<string, boolean> = {}
    const cmd = os.platform() === 'win32' ? 'where' : 'which'
    const env = getLoginEnv() ?? (process.env as Record<string, string>)

    const checks = this.getAll().map(
      (tool) =>
        new Promise<void>((resolve) => {
          // Browser tool is always available (no binary needed)
          if (tool.id === 'browser') {
            result[tool.id] = true
            resolve()
            return
          }
          const binary = this.resolveCommand(tool)
          execFile(cmd, [binary], { env }, (err) => {
            result[tool.id] = !err
            resolve()
          })
        }),
    )

    await Promise.all(checks)
    return result
  }
}
