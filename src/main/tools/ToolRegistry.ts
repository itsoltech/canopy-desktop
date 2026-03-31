import { execFile } from 'child_process'
import { app } from 'electron'
import fs from 'fs'
import os from 'os'
import path from 'path'
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

  removeCustom(id: string): void {
    this.db.prepare('DELETE FROM tool_definitions WHERE id = ? AND is_custom = 1').run(id)
    this.removeIcon(id)
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

  private get iconDir(): string {
    const dir = path.join(app.getPath('userData'), 'tool-icons')
    if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true })
    return dir
  }

  sanitizeSvg(svg: string): string {
    return svg
      .replace(/<script[\s>][\s\S]*?<\/script\s*>/gi, '')
      .replace(/<foreignObject[\s>][\s\S]*?<\/foreignObject\s*>/gi, '')
      .replace(/<style[\s>][\s\S]*?<\/style\s*>/gi, '')
      .replace(/\s+on\w+\s*=\s*"[^"]*"/gi, '')
      .replace(/\s+on\w+\s*=\s*'[^']*'/gi, '')
      .replace(/\s+on\w+\s*=\s*[^\s>]*/gi, '')
      .replace(/(?:xlink:)?href\s*=\s*["']?\s*javascript:/gi, 'href="')
      .replace(/(?:xlink:)?href\s*=\s*["']?\s*data:/gi, 'href="')
  }

  setIcon(toolId: string, svgContent: string): void {
    this.validateId(toolId)
    const filePath = path.join(this.iconDir, `${toolId}.svg`)
    fs.writeFileSync(filePath, this.sanitizeSvg(svgContent), 'utf-8')
  }

  getIcon(toolId: string): string | null {
    this.validateId(toolId)
    const filePath = path.join(this.iconDir, `${toolId}.svg`)
    if (!fs.existsSync(filePath)) return null
    return fs.readFileSync(filePath, 'utf-8')
  }

  removeIcon(toolId: string): void {
    this.validateId(toolId)
    const filePath = path.join(this.iconDir, `${toolId}.svg`)
    if (fs.existsSync(filePath)) fs.unlinkSync(filePath)
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
