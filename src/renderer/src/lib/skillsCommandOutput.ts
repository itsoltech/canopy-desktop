const RESET = '\x1b[0m'
const GREEN = '\x1b[32m'
const CYAN = '\x1b[36m'
const RED = '\x1b[31m'
const DIM = '\x1b[2m'
const BOLD = '\x1b[1m'

interface SkillInfo {
  id: string
  name: string
  description: string
  agents: string[]
  enabledAgents: string[]
  scope: string
  sourceType: string
  sourceUri: string
  installMethod: string
  version: string
}

export function formatSkillList(skills: SkillInfo[]): string {
  if (skills.length === 0) {
    return `${DIM}No skills installed.${RESET}\r\n`
  }

  let output = `${BOLD}Installed Skills${RESET}\r\n\r\n`
  for (const skill of skills) {
    const agents = skill.enabledAgents.join(', ')
    output += `  ${GREEN}${skill.name}${RESET} ${DIM}(${skill.id})${RESET}\r\n`
    output += `    ${CYAN}Agents:${RESET} ${agents}  ${CYAN}Scope:${RESET} ${skill.scope}  ${CYAN}Source:${RESET} ${skill.sourceType}\r\n`
    if (skill.description) {
      output += `    ${DIM}${skill.description}${RESET}\r\n`
    }
    output += `\r\n`
  }
  return output
}

export function formatInstallSuccess(skill: SkillInfo): string {
  return `${GREEN}Installed${RESET} ${BOLD}${skill.name}${RESET} for ${skill.enabledAgents.join(', ')} (${skill.scope})\r\n`
}

export function formatRemoveSuccess(id: string): string {
  return `${GREEN}Removed${RESET} skill ${BOLD}${id}${RESET}\r\n`
}

export function formatUpdateSuccess(skill: SkillInfo): string {
  return `${GREEN}Updated${RESET} ${BOLD}${skill.name}${RESET} to v${skill.version}\r\n`
}

export function formatSkillInfo(skill: SkillInfo): string {
  let output = `${BOLD}${skill.name}${RESET} ${DIM}v${skill.version}${RESET}\r\n\r\n`
  output += `  ${CYAN}ID:${RESET}          ${skill.id}\r\n`
  output += `  ${CYAN}Description:${RESET} ${skill.description || '(none)'}\r\n`
  output += `  ${CYAN}Agents:${RESET}      ${skill.agents.join(', ')}\r\n`
  output += `  ${CYAN}Enabled:${RESET}     ${skill.enabledAgents.join(', ')}\r\n`
  output += `  ${CYAN}Scope:${RESET}       ${skill.scope}\r\n`
  output += `  ${CYAN}Source:${RESET}      ${skill.sourceType} — ${skill.sourceUri}\r\n`
  output += `  ${CYAN}Method:${RESET}      ${skill.installMethod}\r\n`
  return output
}

export function formatError(message: string): string {
  return `${RED}Error:${RESET} ${message}\r\n`
}
