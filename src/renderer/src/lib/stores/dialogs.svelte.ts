export type { CrashReportData } from '../../../../renderer-shared/crashReport'

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

interface InputDialogState {
  type: 'input'
  props: PromptOptions & { onSubmit: (result: PromptResult) => void; onCancel: () => void }
}

interface CreateWorktreeState {
  type: 'createWorktree'
  repoRoot?: string
  workspaceId?: string
  baseBranch?: string
}

interface PreferencesState {
  type: 'preferences'
  section?: string
}

interface TaskPickerState {
  type: 'taskPicker'
  connectionId: string
  /** 'browse' (default) → picking a task opens branch creation; 'link' → picking a task links it
   *  to the current worktree (activeTask) without creating anything. */
  mode?: 'browse' | 'link'
}

interface ProjectTrackerState {
  type: 'projectTracker'
}

interface AboutState {
  type: 'about'
}

interface ChangelogState {
  type: 'changelog'
  fromVersion: string
}

interface OnboardingWizardState {
  type: 'onboardingWizard'
}

interface FeatureOnboardingState {
  type: 'featureOnboarding'
  fromVersion: string
}

interface TmuxBrowserState {
  type: 'tmuxBrowser'
}

interface CreateGitHubPRState {
  type: 'createGitHubPR'
}

interface RemoteAcceptDeviceState {
  type: 'remoteAcceptDevice'
  deviceId: string
  deviceName: string
  fingerprint: string
}

interface RunConfigEditorState {
  type: 'runConfigEditor'
  configDir: string
  configName?: string
}

interface RunConfigManagerState {
  type: 'runConfigManager'
  selectConfigDir?: string
  selectConfigName?: string
}

interface CrashReportState {
  type: 'crashReport'
  data: CrashReportData
}

interface PRDetailsState {
  type: 'prDetails'
  repoRoot: string
  branch: string
}

interface CreateTaskPRState {
  type: 'createTaskPR'
  repoRoot: string
  branch: string
  task: { taskKey: string; summary: string; connectionId?: string; boardId?: string }
}

interface NoneState {
  type: 'none'
}

type DialogState =
  | NoneState
  | InputDialogState
  | CreateWorktreeState
  | PreferencesState
  | TaskPickerState
  | ProjectTrackerState
  | PRDetailsState
  | CreateTaskPRState
  | AboutState
  | ChangelogState
  | OnboardingWizardState
  | FeatureOnboardingState
  | TmuxBrowserState
  | CreateGitHubPRState
  | RemoteAcceptDeviceState
  | RunConfigEditorState
  | RunConfigManagerState
  | CrashReportState

export const dialogState: { current: DialogState } = $state({ current: { type: 'none' } })

// Confirmations render ABOVE whatever dialog is open (separate stacked state), so asking for a
// confirmation inside a modal (e.g. the Project tracker dialog) doesn't replace and close it.
export const confirmState: {
  current: (ConfirmOptions & { onConfirm: () => void; onCancel: () => void }) | null
} = $state({ current: null })

export function confirm(opts: ConfirmOptions): Promise<boolean> {
  return new Promise((resolve) => {
    confirmState.current = {
      ...opts,
      onConfirm: () => {
        confirmState.current = null
        resolve(true)
      },
      onCancel: () => {
        confirmState.current = null
        resolve(false)
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

export function showCreateWorktree(opts?: {
  repoRoot?: string
  workspaceId?: string
  baseBranch?: string
}): void {
  dialogState.current = {
    type: 'createWorktree',
    repoRoot: opts?.repoRoot,
    workspaceId: opts?.workspaceId,
    baseBranch: opts?.baseBranch,
  }
}

export function showPreferences(section?: string): void {
  dialogState.current = { type: 'preferences', section }
}

export function showTaskPicker(connectionId: string, mode: 'browse' | 'link' = 'browse'): void {
  dialogState.current = { type: 'taskPicker', connectionId, mode }
}

export function showProjectTracker(): void {
  dialogState.current = { type: 'projectTracker' }
}

/** Native PR panel — details fetched via the authenticated gh CLI, no browser login needed. */
export function showPRDetails(repoRoot: string, branch: string): void {
  dialogState.current = { type: 'prDetails', repoRoot, branch }
}

/** Native create-PR form: template-rendered title/body editable before anything is created. */
export function showCreateTaskPR(
  repoRoot: string,
  branch: string,
  task: { taskKey: string; summary: string; connectionId?: string; boardId?: string },
): void {
  dialogState.current = { type: 'createTaskPR', repoRoot, branch, task }
}

export function showAbout(): void {
  dialogState.current = { type: 'about' }
}

export function showChangelog(fromVersion: string): void {
  dialogState.current = { type: 'changelog', fromVersion }
}

export function showOnboardingWizard(): void {
  dialogState.current = { type: 'onboardingWizard' }
}

export function showFeatureOnboarding(fromVersion: string): void {
  dialogState.current = { type: 'featureOnboarding', fromVersion }
}

export function showTmuxBrowser(): void {
  dialogState.current = { type: 'tmuxBrowser' }
}

export function showCreateGitHubPR(): void {
  dialogState.current = { type: 'createGitHubPR' }
}

export function showRemoteAcceptDevice(device: {
  deviceId: string
  deviceName: string
  fingerprint: string
}): void {
  dialogState.current = {
    type: 'remoteAcceptDevice',
    deviceId: device.deviceId,
    deviceName: device.deviceName,
    fingerprint: device.fingerprint,
  }
}

export function showRunConfigEditor(configDir: string, configName?: string): void {
  dialogState.current = { type: 'runConfigEditor', configDir, configName }
}

export function showRunConfigManager(configDir?: string, configName?: string): void {
  dialogState.current = {
    type: 'runConfigManager',
    selectConfigDir: configDir,
    selectConfigName: configName,
  }
}

export function showCrashReport(data: CrashReportData): void {
  dialogState.current = { type: 'crashReport', data }
}

export function closeDialog(): void {
  dialogState.current = { type: 'none' }
}
