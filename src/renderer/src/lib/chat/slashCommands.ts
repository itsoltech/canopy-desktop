import { match } from 'ts-pattern'

/**
 * Supported inline slash commands parsed at submit time in SdkChatPane.
 * Anything not matching this list falls through as plain text.
 */
export type SlashCommand =
  | { kind: 'new' }
  | { kind: 'clear' }
  | { kind: 'retry' }
  | { kind: 'model'; model: string }
  | { kind: 'mode'; mode: 'default' | 'plan' | 'acceptEdits' | 'bypassPermissions' }
  | { kind: 'none'; text: string }
  | { kind: 'invalid'; reason: string }

const VALID_MODES: ReadonlySet<string> = new Set([
  'default',
  'plan',
  'acceptEdits',
  'bypassPermissions',
])

export function parseSlashCommand(input: string): SlashCommand {
  const text = input.trim()
  if (!text.startsWith('/')) return { kind: 'none', text }
  const [head, ...rest] = text.slice(1).split(/\s+/)
  const arg = rest.join(' ').trim()
  return match(head)
    .with('new', () => ({ kind: 'new' }) as SlashCommand)
    .with('clear', () => ({ kind: 'clear' }) as SlashCommand)
    .with('retry', () => ({ kind: 'retry' }) as SlashCommand)
    .with('model', () =>
      arg.length > 0
        ? ({ kind: 'model', model: arg } as SlashCommand)
        : ({ kind: 'invalid', reason: '/model needs a model name' } as SlashCommand),
    )
    .with('mode', () =>
      VALID_MODES.has(arg)
        ? ({
            kind: 'mode',
            mode: arg as 'default' | 'plan' | 'acceptEdits' | 'bypassPermissions',
          } as SlashCommand)
        : ({
            kind: 'invalid',
            reason: `/mode must be one of: ${[...VALID_MODES].join(', ')}`,
          } as SlashCommand),
    )
    .otherwise(() => ({ kind: 'none', text }) as SlashCommand)
}

export interface SlashCommandHintSpec {
  command: string
  description: string
}

export const SLASH_COMMAND_HINTS: readonly SlashCommandHintSpec[] = [
  { command: '/new', description: 'Start a fresh conversation in this pane.' },
  { command: '/clear', description: 'Clear the visible transcript (DB untouched).' },
  { command: '/retry', description: 'Resend the last user message.' },
  { command: '/model <name>', description: 'Override the model for the next message.' },
  {
    command: '/mode <default|plan|acceptEdits|bypassPermissions>',
    description: 'Change permission mode for the next message.',
  },
]
