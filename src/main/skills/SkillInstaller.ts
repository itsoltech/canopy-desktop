import { execFile } from 'child_process'
import { readFileSync, existsSync, statSync, readdirSync } from 'fs'
import { join, basename } from 'path'
import { tmpdir } from 'os'
import { mkdtempSync, rmSync } from 'fs'
import { ok, err } from '../errors'
import type { Result } from 'neverthrow'
import type { SkillError } from './errors'
import type { CanopySkill, SkillInstallOptions, SkillAgentTarget } from './types'
import { parseSkillContent } from './SkillParser'
import { SkillStore } from './SkillStore'
import { getTransformer } from './SkillTransformer'

interface SourceResolution {
  content: string
  fileName: string
  sourceType: 'github' | 'url' | 'local'
  sourceUri: string
}

export class SkillInstaller {
  constructor(private store: SkillStore) {}

  async install(opts: SkillInstallOptions): Promise<Result<CanopySkill, SkillError>> {
    // 1. Resolve source
    const resolved = await this.resolveSource(opts.source)
    if (resolved.isErr()) return err(resolved.error)

    const { content, fileName, sourceType, sourceUri } = resolved.value

    // 2. Parse content
    const parsed = parseSkillContent(content, sourceUri, fileName)
    if (parsed.isErr()) return err(parsed.error)

    const skillData = parsed.value

    // 3. Determine ID
    const id = skillData.name
      .toLowerCase()
      .replace(/[^a-z0-9-]/g, '-')
      .replace(/-+/g, '-')

    if (this.store.exists(id)) {
      return err({ _tag: 'SkillAlreadyInstalled', skillId: id })
    }

    // 4. Determine agents
    const agents: SkillAgentTarget[] = opts.agents?.length
      ? opts.agents
      : skillData.agents.length
        ? skillData.agents
        : ['claude']

    // 5. Build CanopySkill
    const skill: CanopySkill = {
      id,
      name: skillData.name,
      description: skillData.description,
      version: skillData.version,
      prompt: skillData.prompt,
      agents,
      metadata: skillData.metadata,
      sourceType,
      sourceUri,
      installMethod: opts.method ?? 'copy',
      scope: opts.scope ?? 'project',
      workspaceId: opts.workspaceId ?? null,
      enabledAgents: agents,
      installedAt: new Date().toISOString(),
    }

    // 6. Save to DB
    this.store.insert(skill)

    // 7. Deploy to each agent directory
    if (opts.workspacePath) {
      for (const agent of skill.enabledAgents) {
        const transformer = getTransformer(agent)
        if (!transformer) continue
        const deployResult = transformer.deploy(skill, opts.workspacePath)
        if (deployResult.isErr()) return err(deployResult.error)
      }
    }

    return ok(skill)
  }

  async update(skillId: string, workspacePath?: string): Promise<Result<CanopySkill, SkillError>> {
    const existing = this.store.get(skillId)
    if (!existing) return err({ _tag: 'SkillNotFound', skillId })

    // Re-fetch from source
    const resolved = await this.resolveSource(existing.sourceUri)
    if (resolved.isErr()) return err(resolved.error)

    const parsed = parseSkillContent(
      resolved.value.content,
      existing.sourceUri,
      resolved.value.fileName,
    )
    if (parsed.isErr()) return err(parsed.error)

    // Remove old
    this.store.remove(skillId)

    // Re-install with same options
    const skill: CanopySkill = {
      ...existing,
      name: parsed.value.name || existing.name,
      description: parsed.value.description || existing.description,
      version: parsed.value.version,
      prompt: parsed.value.prompt,
      metadata: { ...existing.metadata, ...parsed.value.metadata },
      installedAt: new Date().toISOString(),
    }

    this.store.insert(skill)

    if (workspacePath) {
      for (const agent of skill.enabledAgents) {
        const transformer = getTransformer(agent)
        if (!transformer) continue
        transformer.deploy(skill, workspacePath)
      }
    }

    return ok(skill)
  }

  remove(skillId: string, workspacePath?: string): Result<void, SkillError> {
    const skill = this.store.get(skillId)
    if (!skill) return err({ _tag: 'SkillNotFound', skillId })

    // Undeploy from all agent directories
    if (workspacePath) {
      for (const agent of skill.enabledAgents) {
        const transformer = getTransformer(agent)
        if (!transformer) continue
        transformer.undeploy(skill, workspacePath)
      }
    }

    this.store.remove(skillId)
    return ok(undefined)
  }

  private async resolveSource(source: string): Promise<Result<SourceResolution, SkillError>> {
    if (source.startsWith('github:')) {
      return this.fetchFromGitHub(source.slice(7))
    }
    if (source.startsWith('http://') || source.startsWith('https://')) {
      return this.fetchFromUrl(source)
    }
    return this.fetchFromLocal(source)
  }

  private async fetchFromGitHub(ref: string): Promise<Result<SourceResolution, SkillError>> {
    const parts = ref.split('/')
    if (parts.length < 2) {
      return err({
        _tag: 'InvalidSource',
        source: `github:${ref}`,
        reason: 'Expected format: owner/repo[/path]',
      })
    }

    const owner = parts[0]
    const repo = parts[1]
    const subpath = parts.slice(2).join('/')

    const tmpDir = mkdtempSync(join(tmpdir(), 'canopy-skill-'))

    try {
      await new Promise<void>((resolve, reject) => {
        execFile(
          'git',
          ['clone', '--depth', '1', `https://github.com/${owner}/${repo}.git`, tmpDir],
          (error) => {
            if (error) reject(error)
            else resolve()
          },
        )
      })

      const skillDir = subpath ? join(tmpDir, subpath) : tmpDir
      return this.readSkillDir(skillDir, `github:${ref}`, 'github')
    } catch (e) {
      return err({
        _tag: 'FetchFailed',
        source: `github:${ref}`,
        cause: e instanceof Error ? e.message : String(e),
      })
    } finally {
      try {
        rmSync(tmpDir, { recursive: true, force: true })
      } catch {
        /* ignore */
      }
    }
  }

  private async fetchFromUrl(url: string): Promise<Result<SourceResolution, SkillError>> {
    try {
      const resp = await fetch(url)
      if (!resp.ok) {
        return err({ _tag: 'FetchFailed', source: url, cause: `HTTP ${resp.status}` })
      }
      const content = await resp.text()
      const fileName = basename(new URL(url).pathname).replace(/\.[^.]+$/, '') || 'skill'
      return ok({ content, fileName, sourceType: 'url', sourceUri: url })
    } catch (e) {
      return err({
        _tag: 'FetchFailed',
        source: url,
        cause: e instanceof Error ? e.message : String(e),
      })
    }
  }

  private fetchFromLocal(path: string): Result<SourceResolution, SkillError> {
    if (!existsSync(path)) {
      return err({ _tag: 'InvalidSource', source: path, reason: 'Path does not exist' })
    }

    const stat = statSync(path)

    if (stat.isDirectory()) {
      return this.readSkillDir(path, path, 'local')
    }

    const content = readFileSync(path, 'utf-8')
    const fileName = basename(path).replace(/\.[^.]+$/, '')
    return ok({ content, fileName, sourceType: 'local', sourceUri: path })
  }

  private readSkillDir(
    dir: string,
    sourceUri: string,
    sourceType: 'github' | 'local',
  ): Result<SourceResolution, SkillError> {
    const candidates = ['SKILL.md', 'canopy-skill.yaml', 'skill.md']
    for (const candidate of candidates) {
      const filePath = join(dir, candidate)
      if (existsSync(filePath)) {
        const content = readFileSync(filePath, 'utf-8')
        const fileName = basename(dir)
        return ok({ content, fileName, sourceType, sourceUri })
      }
    }

    const files = readdirSync(dir).filter((f) => f.endsWith('.md'))
    if (files.length > 0) {
      const content = readFileSync(join(dir, files[0]), 'utf-8')
      const fileName = basename(dir)
      return ok({ content, fileName, sourceType, sourceUri })
    }

    return err({
      _tag: 'ParseFailed',
      source: sourceUri,
      reason: 'No skill file found in directory',
    })
  }
}
