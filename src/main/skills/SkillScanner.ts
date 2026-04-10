import { readdir, readFile, access } from 'fs/promises'
import { join, basename } from 'path'
import os from 'os'
import { is } from '@electron-toolkit/utils'
import { parseSkillContent } from './SkillParser'

interface ScanTarget {
  agent: 'claude' | 'gemini' | 'cursor' | 'opencode'
  projectDir: string
  globalDir: string
  extensions: string[]
}

const SCAN_TARGETS: ScanTarget[] = [
  {
    agent: 'claude',
    projectDir: '.claude/commands',
    globalDir: join(os.homedir(), '.claude', 'commands'),
    extensions: ['.md'],
  },
  {
    agent: 'gemini',
    projectDir: '.gemini/skills',
    globalDir: join(os.homedir(), '.gemini', 'skills'),
    extensions: ['.md'],
  },
  {
    agent: 'cursor',
    projectDir: '.cursor/rules',
    globalDir: join(os.homedir(), '.cursor', 'rules'),
    extensions: ['.md', '.mdc'],
  },
  {
    agent: 'opencode',
    projectDir: '.opencode/skills',
    globalDir: join(os.homedir(), '.opencode', 'skills'),
    extensions: ['.md'],
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
    // Scan project directory
    if (workspacePath) {
      const projectDir = join(workspacePath, target.projectDir)
      found.push(...(await scanDirectory(projectDir, target.agent, 'project', target.extensions)))
    }

    // Scan global directory
    found.push(
      ...(await scanDirectory(target.globalDir, target.agent, 'global', target.extensions)),
    )
  }

  return found
}

async function scanDirectory(
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
    // Filesystem boundary: directory listing may fail for permission or access reasons
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
