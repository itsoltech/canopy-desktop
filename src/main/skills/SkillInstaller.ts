import { execFile } from 'child_process'
import { mkdtemp, readFile, stat, readdir, access, rm } from 'fs/promises'
import { join, basename, resolve, normalize, extname, sep } from 'path'
import { tmpdir, homedir } from 'os'
import { ok, err, fromExternalCall } from '../errors'
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

    // 6. Deploy to each agent directory (before saving to DB — partial deploy must not persist)
    // Global skills deploy to globalDir (no workspacePath needed); project skills require it
    const targetRoot = opts.workspacePath ?? (skill.scope === 'global' ? '' : null)
    if (targetRoot === null && skill.scope === 'project') {
      return err({
        _tag: 'InstallFailed',
        skillId: skill.id,
        reason: 'workspacePath is required for project-scoped skill installation',
      })
    }
    if (targetRoot !== null) {
      const deployedAgents: typeof skill.enabledAgents = []
      for (const agent of skill.enabledAgents) {
        const transformer = getTransformer(agent)
        if (!transformer) continue
        const deployResult = await transformer.deploy(skill, targetRoot)
        if (deployResult.isErr()) {
          for (const deployed of deployedAgents) {
            const t = getTransformer(deployed)
            if (t) await t.undeploy(skill, targetRoot)
          }
          return err(deployResult.error)
        }
        deployedAgents.push(agent)
      }
    }

    // 7. Save to DB only after all deploys succeed
    this.store.insert(skill)

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

    // Build updated skill
    const skill: CanopySkill = {
      ...existing,
      name: parsed.value.name || existing.name,
      description: parsed.value.description || existing.description,
      version: parsed.value.version,
      prompt: parsed.value.prompt,
      metadata: { ...existing.metadata, ...parsed.value.metadata },
      installedAt: new Date().toISOString(),
    }

    // Deploy before updating DB
    const updateTarget = workspacePath ?? (existing.scope === 'global' ? '' : null)
    if (updateTarget === null && existing.scope === 'project') {
      return err({
        _tag: 'InstallFailed',
        skillId: skill.id,
        reason: 'workspacePath is required for project-scoped skill update',
      })
    }
    if (updateTarget !== null) {
      const deployedAgents: typeof skill.enabledAgents = []
      for (const agent of skill.enabledAgents) {
        const transformer = getTransformer(agent)
        if (!transformer) continue
        const deployResult = await transformer.deploy(skill, updateTarget)
        if (deployResult.isErr()) {
          for (const deployed of deployedAgents) {
            const t = getTransformer(deployed)
            if (t) await t.undeploy(skill, updateTarget)
          }
          return err(deployResult.error)
        }
        deployedAgents.push(agent)
      }
    }

    this.store.update(skill)

    return ok(skill)
  }

  async remove(skillId: string, workspacePath?: string): Promise<Result<void, SkillError>> {
    const skill = this.store.get(skillId)
    if (!skill) return err({ _tag: 'SkillNotFound', skillId })

    // Undeploy from all agent directories
    const removeTarget = workspacePath ?? (skill.scope === 'global' ? '' : null)
    if (removeTarget !== null) {
      for (const agent of skill.enabledAgents) {
        const transformer = getTransformer(agent)
        if (!transformer) continue
        await transformer.undeploy(skill, removeTarget)
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
    return await this.fetchFromLocal(source)
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

    const validName = /^[a-zA-Z0-9._-]+$/
    if (!validName.test(owner) || !validName.test(repo)) {
      return err({
        _tag: 'InvalidSource',
        source: `github:${ref}`,
        reason: 'Invalid GitHub owner or repo name',
      })
    }

    const tmpDir = await mkdtemp(join(tmpdir(), 'canopy-skill-'))

    try {
      const cloneResult = await fromExternalCall(
        new Promise<void>((resolve, reject) => {
          execFile(
            'git',
            ['clone', '--depth', '1', `https://github.com/${owner}/${repo}.git`, tmpDir],
            (error) => {
              if (error) reject(error)
              else resolve()
            },
          )
        }),
        (e): SkillError => ({
          _tag: 'FetchFailed',
          source: `github:${ref}`,
          cause: e instanceof Error ? e.message : String(e),
        }),
      )

      if (cloneResult.isErr()) return err(cloneResult.error)

      const skillDir = subpath ? join(tmpDir, subpath) : tmpDir
      return await this.readSkillDir(skillDir, `github:${ref}`, 'github')
    } finally {
      // Temp dir cleanup is allowed in finally blocks (CLAUDE.md)
      await rm(tmpDir, { recursive: true, force: true }).catch(() => {})
    }
  }

  private async fetchFromUrl(url: string): Promise<Result<SourceResolution, SkillError>> {
    const fetchResult = await fromExternalCall(
      fetch(url),
      (e): SkillError => ({
        _tag: 'FetchFailed',
        source: url,
        cause: e instanceof Error ? e.message : String(e),
      }),
    )

    if (fetchResult.isErr()) return err(fetchResult.error)

    const resp = fetchResult.value
    if (!resp.ok) {
      return err({ _tag: 'FetchFailed', source: url, cause: `HTTP ${resp.status}` })
    }

    const textResult = await fromExternalCall(
      resp.text(),
      (e): SkillError => ({
        _tag: 'FetchFailed',
        source: url,
        cause: e instanceof Error ? e.message : String(e),
      }),
    )

    if (textResult.isErr()) return err(textResult.error)

    const content = textResult.value
    const fileName = basename(new URL(url).pathname).replace(/\.[^.]+$/, '') || 'skill'
    return ok({ content, fileName, sourceType: 'url', sourceUri: url })
  }

  private async fetchFromLocal(localPath: string): Promise<Result<SourceResolution, SkillError>> {
    const resolved = normalize(resolve(localPath))
    const home = normalize(homedir()) + sep
    const tmp = normalize(tmpdir()) + sep
    if (!resolved.startsWith(home) && !resolved.startsWith(tmp)) {
      return err({
        _tag: 'InvalidSource',
        source: localPath,
        reason: 'Path must be within home directory or temp directory',
      })
    }

    // Allowlist: dotfile directories must be known agent config paths (only for home paths)
    if (resolved.startsWith(home)) {
      const relativeToHome = resolved.slice(home.length)
      const firstSegment = relativeToHome.split(sep)[0]
      if (firstSegment.startsWith('.')) {
        const allowedDotDirs = ['.claude', '.gemini', '.cursor', '.opencode', '.agents']
        if (!allowedDotDirs.includes(firstSegment)) {
          return err({
            _tag: 'InvalidSource',
            source: localPath,
            reason:
              'Only agent config directories (.claude, .gemini, .cursor, .opencode, .agents) are allowed within dotfiles',
          })
        }
      }
    }

    let fileStat: Awaited<ReturnType<typeof stat>>
    try {
      fileStat = await stat(resolved)
    } catch {
      return err({ _tag: 'InvalidSource', source: localPath, reason: 'Path does not exist' })
    }

    if (fileStat.isDirectory()) {
      return await this.readSkillDir(resolved, resolved, 'local')
    }

    // Validate file extension for single files
    const ext = extname(localPath).toLowerCase()
    if (!['.md', '.yaml', '.yml', '.mdc'].includes(ext)) {
      return err({
        _tag: 'InvalidSource',
        source: localPath,
        reason: 'Only .md, .yaml, .yml, and .mdc files are supported',
      })
    }

    const readResult = await fromExternalCall(
      readFile(resolved, 'utf-8'),
      (e): SkillError => ({
        _tag: 'FetchFailed',
        source: resolved,
        cause: e instanceof Error ? e.message : String(e),
      }),
    )
    if (readResult.isErr()) return err(readResult.error)
    const fileName = basename(resolved).replace(/\.[^.]+$/, '')
    return ok({ content: readResult.value, fileName, sourceType: 'local', sourceUri: resolved })
  }

  private async readSkillDir(
    dir: string,
    sourceUri: string,
    sourceType: 'github' | 'local',
  ): Promise<Result<SourceResolution, SkillError>> {
    const candidates = ['SKILL.md', 'canopy-skill.yaml', 'skill.md']
    for (const candidate of candidates) {
      const filePath = join(dir, candidate)
      try {
        await access(filePath)
        const content = await readFile(filePath, 'utf-8')
        const fileName = basename(dir)
        return ok({ content, fileName, sourceType, sourceUri })
      } catch {
        continue
      }
    }

    const readdirResult = await fromExternalCall(
      readdir(dir),
      (e): SkillError => ({
        _tag: 'FetchFailed',
        source: sourceUri,
        cause: e instanceof Error ? e.message : String(e),
      }),
    )
    if (readdirResult.isErr()) return err(readdirResult.error)
    const allFiles = readdirResult.value
    const files = allFiles.filter((f) => f.endsWith('.md'))
    if (files.length > 0) {
      const readResult = await fromExternalCall(
        readFile(join(dir, files[0]), 'utf-8'),
        (e): SkillError => ({
          _tag: 'FetchFailed',
          source: sourceUri,
          cause: e instanceof Error ? e.message : String(e),
        }),
      )
      if (readResult.isErr()) return err(readResult.error)
      const fileName = basename(dir)
      return ok({ content: readResult.value, fileName, sourceType, sourceUri })
    }

    return err({
      _tag: 'ParseFailed',
      source: sourceUri,
      reason: 'No skill file found in directory',
    })
  }
}
