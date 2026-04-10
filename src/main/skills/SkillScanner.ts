import { readdirSync, readFileSync, existsSync } from 'fs'
import { join, basename } from 'path'
import os from 'os'
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

export function scanSkills(workspacePath?: string): ScannedSkill[] {
  const found: ScannedSkill[] = []

  for (const target of SCAN_TARGETS) {
    // Scan project directory
    if (workspacePath) {
      const projectDir = join(workspacePath, target.projectDir)
      found.push(...scanDirectory(projectDir, target.agent, 'project', target.extensions))
    }

    // Scan global directory
    found.push(...scanDirectory(target.globalDir, target.agent, 'global', target.extensions))
  }

  return found
}

function scanDirectory(
  dir: string,
  agent: string,
  scope: 'project' | 'global',
  extensions: string[],
): ScannedSkill[] {
  if (!existsSync(dir)) return []

  const results: ScannedSkill[] = []

  try {
    const files = readdirSync(dir).filter((f) => extensions.some((ext) => f.endsWith(ext)))

    for (const file of files) {
      const filePath = join(dir, file)
      try {
        const content = readFileSync(filePath, 'utf-8')
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
      } catch {
        // Skip unreadable files
      }
    }
  } catch {
    // Skip unreadable directories
  }

  return results
}
