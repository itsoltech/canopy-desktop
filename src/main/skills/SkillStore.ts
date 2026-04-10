import type { Database as BetterSqlite3Database } from 'better-sqlite3'
import type { Database } from '../db/Database'
import type { SkillDefinitionRow } from '../db/types'
import { skillFromRow } from '../db/types'
import type { CanopySkill, SkillListOptions } from './types'

export class SkillStore {
  constructor(private database: Database) {}

  private get db(): BetterSqlite3Database {
    return this.database.db
  }

  getAll(): CanopySkill[] {
    const rows = this.db
      .prepare('SELECT * FROM skill_definitions ORDER BY installed_at DESC')
      .all() as SkillDefinitionRow[]
    return JSON.parse(JSON.stringify(rows.map(skillFromRow)))
  }

  get(id: string): CanopySkill | undefined {
    const row = this.db.prepare('SELECT * FROM skill_definitions WHERE id = ?').get(id) as
      | SkillDefinitionRow
      | undefined
    return row ? JSON.parse(JSON.stringify(skillFromRow(row))) : undefined
  }

  list(opts?: SkillListOptions): CanopySkill[] {
    let sql = 'SELECT * FROM skill_definitions WHERE 1=1'
    const params: unknown[] = []

    if (opts?.scope) {
      sql += ' AND scope = ?'
      params.push(opts.scope)
    }
    if (opts?.workspaceId !== undefined) {
      if (opts.workspaceId === null) {
        sql += ' AND workspace_id IS NULL'
      } else {
        sql += ' AND workspace_id = ?'
        params.push(opts.workspaceId)
      }
    }

    sql += ' ORDER BY installed_at DESC'

    const rows = this.db.prepare(sql).all(...params) as SkillDefinitionRow[]
    let skills: CanopySkill[] = JSON.parse(JSON.stringify(rows.map(skillFromRow)))

    if (opts?.agent) {
      skills = skills.filter((s) => s.agents.includes(opts.agent!))
    }

    return skills
  }

  insert(skill: CanopySkill): void {
    this.db
      .prepare(
        `INSERT INTO skill_definitions (id, name, description, version, prompt, agents_json, metadata_json, source_type, source_uri, install_method, scope, workspace_id, enabled_agents_json)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      )
      .run(
        skill.id,
        skill.name,
        skill.description,
        skill.version,
        skill.prompt,
        JSON.stringify(skill.agents),
        JSON.stringify(skill.metadata),
        skill.sourceType,
        skill.sourceUri,
        skill.installMethod,
        skill.scope,
        skill.workspaceId,
        JSON.stringify(skill.enabledAgents),
      )
  }

  update(skill: CanopySkill): void {
    this.db
      .prepare(
        `UPDATE skill_definitions SET name = ?, description = ?, version = ?, prompt = ?,
         agents_json = ?, metadata_json = ?, enabled_agents_json = ?, installed_at = ?
         WHERE id = ?`,
      )
      .run(
        skill.name,
        skill.description,
        skill.version,
        skill.prompt,
        JSON.stringify(skill.agents),
        JSON.stringify(skill.metadata),
        JSON.stringify(skill.enabledAgents),
        skill.installedAt,
        skill.id,
      )
  }

  remove(id: string): boolean {
    const result = this.db.prepare('DELETE FROM skill_definitions WHERE id = ?').run(id)
    return result.changes > 0
  }

  updateEnabledAgents(id: string, enabledAgents: string[]): void {
    this.db
      .prepare('UPDATE skill_definitions SET enabled_agents_json = ? WHERE id = ?')
      .run(JSON.stringify(enabledAgents), id)
  }

  exists(id: string): boolean {
    const row = this.db.prepare('SELECT 1 FROM skill_definitions WHERE id = ?').get(id)
    return row !== undefined
  }
}
