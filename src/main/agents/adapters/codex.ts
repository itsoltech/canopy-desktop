import { match, P } from 'ts-pattern'
import { readFileSync, writeFileSync, existsSync, mkdirSync, unlinkSync, rmdirSync } from 'fs'
import { join } from 'path'
import type {
  AgentAdapter,
  AgentType,
  NormalizedEventName,
  NormalizedHookEvent,
  NormalizedStatusData,
  PreferencesReader,
  SettingsSetup,
} from '../types'
import type { SessionStatusType } from '../../notch/types'
import { BLOCKED_ENV_VARS } from '../../security/envBlocklist'
import { summarizeToolInput } from '../utils'

const CODEX_HOOK_EVENTS = ['SessionStart', 'UserPromptSubmit', 'PreToolUse', 'PostToolUse', 'Stop']

const EVENT_MAP: Record<string, NormalizedEventName> = {
  SessionStart: 'SessionStart',
  UserPromptSubmit: 'PromptSubmit',
  PreToolUse: 'BeforeToolUse',
  PostToolUse: 'AfterToolUse',
  Stop: 'Idle',
}

const INTERNAL_BLOCKED = new Set(['CANOPY_HOOK_PORT', 'CANOPY_HOOK_TOKEN', 'ELECTRON_RUN_AS_NODE'])

/** Refcount for concurrent Codex sessions sharing the same worktree hooks.json */
interface WorktreeRef {
  count: number
  original: string | null
  createdDir: boolean
}
const worktreeRefs = new Map<string, WorktreeRef>()

function deepMerge(
  target: Record<string, unknown>,
  source: Record<string, unknown>,
): Record<string, unknown> {
  const out = { ...target }
  for (const [key, val] of Object.entries(source)) {
    if (
      val !== null &&
      typeof val === 'object' &&
      !Array.isArray(val) &&
      typeof out[key] === 'object' &&
      out[key] !== null &&
      !Array.isArray(out[key])
    ) {
      out[key] = deepMerge(out[key] as Record<string, unknown>, val as Record<string, unknown>)
    } else {
      out[key] = val
    }
  }
  return out
}

export const codexAdapter: AgentAdapter = {
  agentType: 'codex' as AgentType,
  toolId: 'codex',

  busyEvents: new Set(['UserPromptSubmit', 'PreToolUse']),
  idleEvents: new Set(['Stop']),

  setupSettings(
    _settingsPath: string,
    worktreePath: string,
    hookScriptPath: string,
    _statusLineScriptPath: string | null,
    overrides?: Record<string, unknown>,
  ): SettingsSetup {
    const codexDir = join(worktreePath, '.codex')
    const hooksPath = join(codexDir, 'hooks.json')

    let ref = worktreeRefs.get(worktreePath)
    if (!ref) {
      const createdDir = !existsSync(codexDir)
      if (createdDir) mkdirSync(codexDir, { recursive: true })

      let original: string | null = null
      if (existsSync(hooksPath)) {
        try {
          original = readFileSync(hooksPath, 'utf-8')
        } catch {
          /* inaccessible */
        }
      }

      ref = { count: 0, original, createdDir }
      worktreeRefs.set(worktreePath, ref)
    }
    ref.count++

    // Build Canopy hook entries
    const canopyHooks: Record<
      string,
      Array<{ matcher: string; hooks: Array<{ type: string; command: string }> }>
    > = {}
    for (const event of CODEX_HOOK_EVENTS) {
      canopyHooks[event] = [{ matcher: '', hooks: [{ type: 'command', command: hookScriptPath }] }]
    }

    // Merge with existing project-level hooks
    let existing: Record<string, unknown> = {}
    if (ref.original) {
      try {
        existing = JSON.parse(ref.original)
      } catch {
        /* corrupt file */
      }
    }

    const merged = deepMerge(existing, { ...(overrides ?? {}), hooks: canopyHooks })
    writeFileSync(hooksPath, JSON.stringify(merged, null, 2), 'utf-8')

    return {
      args: ['--enable', 'codex_hooks'],
      cleanup: () => {
        const r = worktreeRefs.get(worktreePath)
        if (!r) return
        r.count--
        if (r.count <= 0) {
          try {
            if (r.original !== null) {
              writeFileSync(hooksPath, r.original, 'utf-8')
            } else {
              unlinkSync(hooksPath)
              if (r.createdDir) {
                rmdirSync(codexDir)
              }
            }
          } catch {
            /* best-effort */
          }
          worktreeRefs.delete(worktreePath)
        }
      },
    }
  },

  normalizeEvent(raw: Record<string, unknown>): NormalizedHookEvent {
    const rawName = (raw.hook_event_name as string) ?? ''

    // Collect Codex-specific extra fields for the inspector
    const extra: Record<string, unknown> = {}
    if (raw.cwd) extra.cwd = raw.cwd
    if (raw.transcript_path) extra.transcriptPath = raw.transcript_path
    if (raw.turn_id) extra.turnId = raw.turn_id
    if (raw.last_assistant_message) extra.lastAssistantMessage = raw.last_assistant_message
    if (raw.source) extra.source = raw.source

    return {
      agentType: 'codex',
      sessionId: (raw.session_id as string) ?? '',
      event: EVENT_MAP[rawName] ?? 'Unknown',
      rawEventName: rawName,
      toolName: raw.tool_name as string | undefined,
      toolInput: raw.tool_input as Record<string, unknown> | undefined,
      toolResponse: raw.tool_response as string | undefined,
      error: raw.error as string | undefined,
      errorDetails: raw.error_details as string | undefined,
      message: raw.message as string | undefined,
      title: raw.title as string | undefined,
      model: raw.model as string | undefined,
      permissionMode: raw.permission_mode as string | undefined,
      prompt: raw.prompt as string | undefined,
      extra: Object.keys(extra).length > 0 ? extra : undefined,
    }
  },

  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  normalizeStatus(_raw: Record<string, unknown>): NormalizedStatusData {
    // Codex does not expose a status line
    return {}
  },

  buildCliArgs(prefs: PreferencesReader): string[] {
    const args: string[] = []
    const model = prefs.get('codex.model')
    const approvalMode = prefs.get('codex.approvalMode')
    const sandbox = prefs.get('codex.sandbox')
    const fullAuto = prefs.get('codex.fullAuto')
    const profile = prefs.get('codex.profile')

    if (model) args.push('--model', model)
    if (approvalMode) args.push('--ask-for-approval', approvalMode)
    if (sandbox) args.push('--sandbox', sandbox)
    if (fullAuto === 'true') args.push('--full-auto')
    if (profile) args.push('--profile', profile)

    return args
  },

  buildEnvVars(prefs: PreferencesReader): Record<string, string> {
    const env: Record<string, string> = {}

    const apiKey = prefs.get('codex.apiKey')
    const baseUrl = prefs.get('codex.baseUrl')
    const customEnv = prefs.get('codex.customEnv')

    if (apiKey) env.OPENAI_API_KEY = apiKey
    if (baseUrl) env.OPENAI_BASE_URL = baseUrl

    if (customEnv) {
      try {
        const parsed = JSON.parse(customEnv)
        for (const [k, v] of Object.entries(parsed)) {
          if (
            typeof v === 'string' &&
            !BLOCKED_ENV_VARS.has(k.toUpperCase()) &&
            !INTERNAL_BLOCKED.has(k)
          ) {
            env[k] = v
          }
        }
      } catch {
        // Invalid JSON
      }
    }

    return env
  },

  buildResumeArgs(resumeSessionId: string): string[] {
    return ['resume', resumeSessionId]
  },

  buildSessionContext(worktreePath: string, workspaceName: string, branch: string | null): string {
    let ctx = `Working in canopy workspace '${workspaceName}'`
    if (branch) {
      ctx += `, worktree '${branch}' (branch: ${branch})`
    }
    ctx += `.\nProject root: ${worktreePath}.`
    return ctx
  },

  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  formatNotification(_event: NormalizedHookEvent): { title: string; body: string } | null {
    // Codex has no interactive permission request events
    return null
  },

  toNotchStatus(event: NormalizedHookEvent): { status: SessionStatusType; detail?: string } | null {
    const toolDetail = event.toolName
      ? `${event.toolName}: ${summarizeToolInput(event.toolInput)}`
      : undefined

    return match(event.event)
      .with(P.union('SessionStart', 'Idle'), () => ({ status: 'idle' as const }))
      .with(P.union('PromptSubmit', 'AfterToolUse'), () => ({ status: 'thinking' as const }))
      .with('BeforeToolUse', () => ({ status: 'toolCalling' as const, detail: toolDetail }))
      .otherwise(() => null)
  },
}
