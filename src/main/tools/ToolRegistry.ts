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

  private validateId(id: string): void {
    if (!id || /[/\\]|\.\.|\0/.test(id)) {
      throw new Error('Invalid tool ID: contains path separators or traversal characters')
    }
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
    this.validateId(tool.id)
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

  /** Return only user-added tools. Used for settings export. */
  listCustom(): ToolDefinition[] {
    return this.getAll().filter((t) => t.isCustom)
  }

  /**
   * Upsert custom tools from an import file, keyed by id. Reuses the
   * validation from addCustom. Does not open a transaction — the caller
   * wraps the full import in one outer transaction.
   */
  upsertCustomForImport(
    tools: {
      id: string
      name: string
      command: string
      args?: string[]
      icon?: string
      category?: string
    }[],
  ): number {
    // WHERE clause prevents a crafted import from hijacking a built-in tool
    // (e.g. id: "shell"). On conflict with a non-custom row, the UPDATE is
    // skipped and SQLite resolves the conflict as a silent no-op.
    const stmt = this.db.prepare(
      `INSERT INTO tool_definitions (id, name, command, args_json, icon, category, is_custom)
       VALUES (?, ?, ?, ?, ?, ?, 1)
       ON CONFLICT(id) DO UPDATE SET
         name = excluded.name,
         command = excluded.command,
         args_json = excluded.args_json,
         icon = excluded.icon,
         category = excluded.category,
         is_custom = 1
       WHERE tool_definitions.is_custom = 1`,
    )
    const SHELL_META = /[;|&$`<>%^!()\\"]/
    let count = 0
    for (const tool of tools) {
      try {
        this.validateId(tool.id)
      } catch {
        continue
      }
      if (!tool.command.trim()) continue
      if (/[/\\;|&$`<>%^!()"]/.test(tool.command)) continue
      if (tool.args?.some((arg) => SHELL_META.test(arg))) continue
      stmt.run(
        tool.id,
        tool.name,
        tool.command,
        JSON.stringify(tool.args ?? []),
        tool.icon ?? 'terminal',
        tool.category ?? 'system',
      )
      count++
    }
    this.reload()
    return count
  }

  removeCustom(id: string): void {
    this.validateId(id)
    this.db.prepare('DELETE FROM tool_definitions WHERE id = ? AND is_custom = 1').run(id)
    this.reload()
  }

  updateCustom(
    id: string,
    changes: {
      name?: string
      command?: string
      args?: string[]
      icon?: string
      category?: string
    },
  ): void {
    this.validateId(id)
    const existing = this.tools.get(id)
    if (!existing || !existing.isCustom) {
      throw new Error(`Custom tool not found: ${id}`)
    }
    if (changes.command !== undefined) {
      if (!changes.command.trim()) {
        throw new Error('Command cannot be empty')
      }
      if (/[/\\;|&$`<>%^!()"]/.test(changes.command)) {
        throw new Error(
          'Invalid command: must be a simple binary name without path separators or shell metacharacters',
        )
      }
    }
    const SHELL_META = /[;|&$`<>%^!()\\"]/
    if (changes.args?.some((arg) => SHELL_META.test(arg))) {
      throw new Error('Invalid args: contain shell metacharacters')
    }
    const setClauses: string[] = []
    const values: unknown[] = []
    if (changes.name !== undefined) {
      setClauses.push('name = ?')
      values.push(changes.name)
    }
    if (changes.command !== undefined) {
      setClauses.push('command = ?')
      values.push(changes.command)
    }
    if (changes.args !== undefined) {
      setClauses.push('args_json = ?')
      values.push(JSON.stringify(changes.args))
    }
    if (changes.icon !== undefined) {
      setClauses.push('icon = ?')
      values.push(changes.icon)
    }
    if (changes.category !== undefined) {
      setClauses.push('category = ?')
      values.push(changes.category)
    }
    if (setClauses.length === 0) return
    values.push(id)
    this.db
      .prepare(
        `UPDATE tool_definitions SET ${setClauses.join(', ')} WHERE id = ? AND is_custom = 1`,
      )
      .run(...values)
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
