import { readdir, readFile, access, stat } from 'fs/promises'
import { join, basename } from 'path'
import os from 'os'
import { is } from '@electron-toolkit/utils'
import { parseSkillContent } from './SkillParser'

interface ScanTarget {
  agent: 'claude' | 'gemini' | 'cursor' | 'opencode'
  /** Directories containing flat skill files (e.g. .cursor/rules/*.md) */
  flatDirs: { project?: string; global?: string; extensions: string[] }[]
  /** Directories containing skill folders with SKILL.md inside (e.g. .claude/skills/verify/SKILL.md) */
  nestedDirs: { project?: string; global?: string }[]
}

const SCAN_TARGETS: ScanTarget[] = [
  {
    agent: 'claude',
    flatDirs: [
      {
        project: '.claude/commands',
        global: join(os.homedir(), '.claude', 'commands'),
        extensions: ['.md'],
      },
    ],
    nestedDirs: [
      {
        project: '.claude/skills',
        global: join(os.homedir(), '.claude', 'skills'),
      },
    ],
  },
  {
    agent: 'gemini',
    flatDirs: [
      {
        project: '.gemini/skills',
        global: join(os.homedir(), '.gemini', 'skills'),
        extensions: ['.md'],
      },
    ],
    nestedDirs: [],
  },
  {
    agent: 'cursor',
    flatDirs: [
      {
        project: '.cursor/rules',
        global: join(os.homedir(), '.cursor', 'rules'),
        extensions: ['.md', '.mdc'],
      },
    ],
    nestedDirs: [],
  },
  {
    agent: 'opencode',
    flatDirs: [
      {
        project: '.opencode/skills',
        global: join(os.homedir(), '.opencode', 'skills'),
        extensions: ['.md'],
      },
    ],
    nestedDirs: [],
  },
]

export interface ScannedSkill {
  id: string
  name: string
  description: string
  agent: string
  scope: 'project' | 'global'
  filePath: string
  prompt: string
}

export async function scanSkills(workspacePath?: string): Promise<ScannedSkill[]> {
  const found: ScannedSkill[] = []

  for (const target of SCAN_TARGETS) {
    // Scan flat directories (files directly in dir)
    for (const flat of target.flatDirs) {
      if (workspacePath && flat.project) {
        found.push(
          ...(await scanFlatDirectory(
            join(workspacePath, flat.project),
            target.agent,
            'project',
            flat.extensions,
          )),
        )
      }
      if (flat.global) {
        found.push(
          ...(await scanFlatDirectory(flat.global, target.agent, 'global', flat.extensions)),
        )
      }
    }

    // Scan nested directories (subdirs with SKILL.md)
    for (const nested of target.nestedDirs) {
      if (workspacePath && nested.project) {
        found.push(
          ...(await scanNestedDirectory(
            join(workspacePath, nested.project),
            target.agent,
            'project',
          )),
        )
      }
      if (nested.global) {
        found.push(...(await scanNestedDirectory(nested.global, target.agent, 'global')))
      }
    }
  }

  // Also scan Claude plugins cache for installed plugins
  const pluginsCache = join(os.homedir(), '.claude', 'plugins', 'cache')
  found.push(...(await scanPluginsCache(pluginsCache)))

  return found
}

/** Scan a flat directory for skill files (e.g. .cursor/rules/*.md) */
async function scanFlatDirectory(
  dir: string,
  agent: string,
  scope: 'project' | 'global',
  extensions: string[],
): Promise<ScannedSkill[]> {
  try {
    await access(dir)
  } catch {
    return []
  }

  const results: ScannedSkill[] = []

  try {
    // Filesystem boundary: directory listing may fail
    const allFiles = await readdir(dir)
    const files = allFiles.filter((f) => extensions.some((ext) => f.endsWith(ext)))

    for (const file of files) {
      const filePath = join(dir, file)
      try {
        // Filesystem boundary: individual files may be unreadable
        const content = await readFile(filePath, 'utf-8')
        const id = basename(file, file.substring(file.lastIndexOf('.')))
        const parsed = parseSkillContent(content, filePath, id)

        if (parsed.isOk()) {
          results.push({
            id,
            name: parsed.value.name || id,
            description: parsed.value.description,
            agent,
            scope,
            filePath,
            prompt: parsed.value.prompt,
          })
        }
      } catch (e) {
        // Filesystem boundary: skip unreadable files
        if (is.dev) console.warn(`[skills] Failed to read ${filePath}:`, e)
      }
    }
  } catch (e) {
    // Filesystem boundary: skip unreadable directories
    if (is.dev) console.warn(`[skills] Failed to read directory ${dir}:`, e)
  }

  return results
}

/** Scan a directory of skill folders, each containing SKILL.md (e.g. .claude/skills/verify/SKILL.md) */
async function scanNestedDirectory(
  dir: string,
  agent: string,
  scope: 'project' | 'global',
): Promise<ScannedSkill[]> {
  try {
    await access(dir)
  } catch {
    return []
  }

  const results: ScannedSkill[] = []

  try {
    const entries = await readdir(dir)
    for (const entry of entries) {
      const entryPath = join(dir, entry)
      try {
        const entryStat = await stat(entryPath)
        if (!entryStat.isDirectory()) continue

        const skillFile = join(entryPath, 'SKILL.md')
        try {
          await access(skillFile)
        } catch {
          continue
        }

        const content = await readFile(skillFile, 'utf-8')
        const parsed = parseSkillContent(content, skillFile, entry)

        if (parsed.isOk()) {
          results.push({
            id: entry,
            name: parsed.value.name || entry,
            description: parsed.value.description,
            agent,
            scope,
            filePath: skillFile,
            prompt: parsed.value.prompt,
          })
        }
      } catch (e) {
        if (is.dev) console.warn(`[skills] Failed to scan ${entryPath}:`, e)
      }
    }
  } catch (e) {
    if (is.dev) console.warn(`[skills] Failed to read directory ${dir}:`, e)
  }

  return results
}

/** Scan Claude plugins cache for installed plugin skills */
async function scanPluginsCache(cacheDir: string): Promise<ScannedSkill[]> {
  try {
    await access(cacheDir)
  } catch {
    return []
  }

  const results: ScannedSkill[] = []

  try {
    // Structure: cache/<org>/<plugin>/<version>/skills/<skill-name>/SKILL.md
    const orgs = await readdir(cacheDir)
    for (const org of orgs) {
      const orgPath = join(cacheDir, org)
      try {
        const orgStat = await stat(orgPath)
        if (!orgStat.isDirectory()) continue

        const plugins = await readdir(orgPath)
        for (const plugin of plugins) {
          const pluginPath = join(orgPath, plugin)
          const pluginStat = await stat(pluginPath)
          if (!pluginStat.isDirectory()) continue

          const versions = await readdir(pluginPath)
          // Use the latest version (last alphabetically)
          const latestVersion = versions.sort().reverse()[0]
          if (!latestVersion) continue

          const skillsDir = join(pluginPath, latestVersion, 'skills')
          const scanned = await scanNestedDirectory(skillsDir, 'claude', 'global')
          for (const skill of scanned) {
            results.push({
              ...skill,
              id: `${plugin}:${skill.id}`,
              name: `${plugin}/${skill.name}`,
            })
          }
        }
      } catch (e) {
        if (is.dev) console.warn(`[skills] Failed to scan plugin org ${orgPath}:`, e)
      }
    }
  } catch (e) {
    if (is.dev) console.warn(`[skills] Failed to scan plugins cache ${cacheDir}:`, e)
  }

  return results
}
