export interface ConfirmOptions {
  title: string
  message: string
  details?: string
  confirmLabel?: string
  destructive?: boolean
}

export interface PromptCheckbox {
  label: string
  checked?: boolean
}

export interface PromptResult {
  value: string
  checked: boolean
}

export interface PromptOptions {
  title: string
  placeholder?: string
  initialValue?: string
  multiline?: boolean
  submitLabel?: string
  validate?: (value: string) => string | null
  onGenerate?: () => Promise<string | null>
  checkbox?: PromptCheckbox
}

interface ConfirmDialogState {
  type: 'confirm'
  props: ConfirmOptions & { onConfirm: () => void; onCancel: () => void }
}

interface InputDialogState {
  type: 'input'
  props: PromptOptions & { onSubmit: (result: PromptResult) => void; onCancel: () => void }
}

interface CreateWorktreeState {
  type: 'createWorktree'
  repoRoot?: string
  workspaceId?: string
}

interface PreferencesState {
  type: 'preferences'
}

interface NoneState {
  type: 'none'
}

type DialogState =
  | NoneState
  | ConfirmDialogState
  | InputDialogState
  | CreateWorktreeState
  | PreferencesState

export const dialogState: { current: DialogState } = $state({ current: { type: 'none' } })

export function confirm(opts: ConfirmOptions): Promise<boolean> {
  return new Promise((resolve) => {
    dialogState.current = {
      type: 'confirm',
      props: {
        ...opts,
        onConfirm: () => {
          dialogState.current = { type: 'none' }
          resolve(true)
        },
        onCancel: () => {
          dialogState.current = { type: 'none' }
          resolve(false)
        },
      },
    }
  })
}

export function prompt(opts: PromptOptions): Promise<PromptResult | null> {
  return new Promise((resolve) => {
    dialogState.current = {
      type: 'input',
      props: {
        ...opts,
        onSubmit: (result: PromptResult) => {
          dialogState.current = { type: 'none' }
          resolve(result)
        },
        onCancel: () => {
          dialogState.current = { type: 'none' }
          resolve(null)
        },
      },
    }
  })
}

export function showCreateWorktree(opts?: { repoRoot?: string; workspaceId?: string }): void {
  dialogState.current = {
    type: 'createWorktree',
    repoRoot: opts?.repoRoot,
    workspaceId: opts?.workspaceId,
  }
}

export function showPreferences(): void {
  dialogState.current = { type: 'preferences' }
}

export function closeDialog(): void {
  dialogState.current = { type: 'none' }
}
