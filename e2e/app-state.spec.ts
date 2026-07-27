import { test, expect } from './fixtures'
import { execFileSync, execSync, spawn } from 'child_process'
import { mkdtemp, readFile, rm, stat, writeFile } from 'fs/promises'
import { homedir } from 'os'
import { join } from 'path'
import type { Page } from '@playwright/test'

let tmpDir: string
let extraTmpPaths: string[] = []

interface AppStateSnapshot {
  workspace: {
    projects: Array<{
      workspace: { path: string }
      repoRoot: string | null
    }>
    workspaceState: {
      selectedWorktreePath: string | null
      isDirty: boolean
    }
  }
  tabs?: {
    tabsByWorktree: Record<
      string,
      Array<{
        id: string
        toolId: string
        worktreePath: string
        focusedPaneId: string
        suspended?: unknown
        rootSplit: {
          type: 'leaf' | 'split'
          id?: string
          direction?: 'horizontal' | 'vertical'
          ratio?: number
          pane?: {
            id: string
            sessionId: string
            wsUrl: string
            toolId: string
            toolName: string
            isRunning?: boolean
            exitCode?: number | null
            detached?: boolean
            inspectorOpen?: boolean
            tmuxSessionName?: string
            filePath?: string
            editorActiveFile?: string
            editorFiles?: Array<{ filePath: string }>
          }
          first?: unknown
          second?: unknown
        }
      }>
    >
    activeTabIdByWorktree: Record<string, string | null>
  }
}

interface AppStateApi {
  getAppState?: () => Promise<AppStateSnapshot>
  onAppStateChanged?: (callback: (snapshot: AppStateSnapshot) => void) => () => void
  setPref?: (key: string, value: string) => Promise<void>
  resizePty?: (sessionId: string, cols: number, rows: number) => Promise<void>
  workspaceSelectWorktree?: (path: string) => Promise<unknown>
  tabOpenTool?: (toolId: string, worktreePath: string) => Promise<{ openedTab?: { id: string } }>
  tabOpenDiff?: (worktreePath: string) => Promise<{ openedTab?: { id: string } }>
  tabOpenSessionTab?: (
    worktreePath: string,
    name: string,
    sessionId: string,
  ) => Promise<{ openedTab?: { id: string } }>
  runConfigSave?: (
    configDir: string,
    config: {
      configurations: Array<{
        name: string
        command: string
        args?: string
      }>
    },
  ) => Promise<void>
  runConfigExecuteCommand?: (
    configDir: string,
    name: string,
    cwd: string,
  ) => Promise<{ sessionId: string; wsUrl: string }>
  tabOpenEditorFile?: (
    worktreePath: string,
    filePath: string,
  ) => Promise<{
    activeTabId: string | null
    openedTab?: { id: string }
  }>
  tabDetachEditorFile?: (
    worktreePath: string,
    paneId: string,
    filePath: string,
  ) => Promise<{
    activeTabId: string | null
    openedTab?: { id: string }
  }>
  tabCloseEditorFile?: (
    worktreePath: string,
    paneId: string,
    filePath: string,
  ) => Promise<{ activeTabId: string | null; closedTabId?: string }>
  tabCloseTab?: (
    worktreePath: string,
    tabId: string,
  ) => Promise<{ activeTabId: string | null; closedTabId?: string; tabs: unknown[] }>
  tabPrepareCloseTab?: (
    worktreePath: string,
    tabId: string,
  ) => Promise<
    | { ok: true }
    | { ok: false; reason: 'cancelled' }
    | { ok: false; reason: 'save-failed'; failedCount: number }
  >
  tabPrepareCloseAllForWorktree?: (
    worktreePath: string,
    options?: { confirmedActiveProcesses?: boolean },
  ) => Promise<
    | { ok: true }
    | { ok: false; reason: 'cancelled' }
    | { ok: false; reason: 'save-failed'; failedCount: number }
    | {
        ok: false
        reason: 'active-processes'
        warnings: Array<{ tabName: string; description: string }>
      }
  >
  tabGetCloseWarning?: (
    worktreePath: string,
    target: { kind: 'tab'; tabId: string } | { kind: 'pane'; tabId: string; paneId: string },
  ) => Promise<{ description: string | null }>
  tabReopenClosedTab?: (worktreePath: string) => Promise<{
    activeTabId: string | null
    openedTab?: { id: string; toolId: string }
  }>
  tabMoveEditorFile?: (
    worktreePath: string,
    paneId: string,
    filePath: string,
    toIndex: number,
  ) => Promise<{ activeTabId: string | null }>
  tabMoveEditorFileBetweenPanes?: (
    worktreePath: string,
    sourcePaneId: string,
    targetPaneId: string,
    filePath: string,
    toIndex: number,
  ) => Promise<{ activeTabId: string | null }>
  tabSetActiveEditorFile?: (
    worktreePath: string,
    paneId: string,
    filePath: string,
  ) => Promise<{ activeTabId: string | null }>
  tabUpdateEditorFileState?: (
    worktreePath: string,
    paneId: string,
    filePath: string,
    patch: Record<string, unknown>,
  ) => Promise<{ activeTabId: string | null }>
  tabSaveEditorFile?: (
    worktreePath: string,
    paneId: string,
    filePath: string,
    options: {
      content: string
      fileLineEnding?: 'LF' | 'CRLF'
      expectedMtimeMs?: number
    },
  ) => Promise<
    | { ok: true; mtimeMs: number; size: number; result: { activeTabId: string | null } }
    | { ok: false; tag: 'StaleWrite'; actualMtimeMs: number }
    | { ok: false; tag: 'WriteFailed' | 'StatFailed'; message: string }
  >
  tabPrepareCloseEditorFile?: (
    worktreePath: string,
    paneId: string,
    filePath: string,
  ) => Promise<
    | { ok: true }
    | { ok: false; reason: 'cancelled' }
    | { ok: false; reason: 'save-failed'; failedCount: number }
  >
  tabLoadEditorFile?: (
    worktreePath: string,
    paneId: string,
    filePath: string,
    options?: { maxBytes?: number },
  ) => Promise<
    | {
        ok: true
        binary: false
        content: string
        truncated: boolean
        size: number
        canWrite: boolean
        mtimeMs: number
        fileLineEnding: 'LF' | 'CRLF'
      }
    | {
        ok: true
        binary: true
        size: number
        canWrite: boolean
        mtimeMs: number
      }
    | { ok: false; tag: 'ReadFailed' | 'StatFailed'; message: string }
  >
  fileTreeReadDir?: (
    dirPath: string,
  ) => Promise<Array<{ name: string; isDirectory: boolean; size: number }>>
  fileTreeCreateFile?: (filePath: string) => Promise<void>
  fileTreeCreateDirectory?: (dirPath: string) => Promise<void>
  fileTreeGetGitStatus?: (
    repoRoot: string,
    worktreePath: string,
  ) => Promise<{
    statuses: Record<string, string>
    changedDirs: string[]
    affectedPaths: string[]
  }>
  changesGetDiff?: (payload: { worktreePath: string }) => Promise<{
    files: Array<{ path: string; status: string }>
  }>
  changesStageFile?: (payload: { worktreePath: string; filePath: string }) => Promise<void>
  changesRevertFile?: (payload: { worktreePath: string; filePath: string }) => Promise<void>
  taskTrackerPrepareBranchFromTask?: (payload: {
    connectionId: string
    task: Record<string, unknown>
    boardId?: string
    branchType?: string
    repoRoot: string
  }) => Promise<{ branchName: string }>
  taskTrackerCreateBranchFromTask?: (payload: {
    connectionId: string
    task: Record<string, unknown>
    boardId?: string
    branchType?: string
    repoRoot: string
    baseBranch: string
    stashBeforeCreate?: boolean
  }) => Promise<{ branchName: string }>
  worktreeRemoveWithBranch?: (payload: {
    repoRoot: string
    worktreePath: string
    branch?: string
    deleteBranch?: boolean
    forceOnFailure?: boolean
  }) => Promise<{
    worktreeRemoved: boolean
    branchDeleted: boolean
    forcedWorktreeRemove: boolean
    forcedBranchDelete: boolean
  }>
  worktreePrepareRemove?: (payload: {
    repoRoot: string
    worktreePath: string
    branch: string
  }) => Promise<{
    hasUncommittedChanges: boolean
    unmergedCommitCount: number
    branchMerged: boolean
    forceRequired: boolean
    canDeleteBranch: boolean
    warnings: string[]
  }>
  worktreeGetMergedBranches?: (payload: {
    repoRoot: string
    branches: string[]
  }) => Promise<{ mergedBranches: string[] }>
  worktreeListBranches?: (payload: {
    repoRoot: string
  }) => Promise<{ local: string[]; remote: string[]; current: string | null }>
  worktreeRefreshBranches?: (payload: {
    repoRoot: string
  }) => Promise<{ local: string[]; remote: string[]; current: string | null }>
  gitBranchPrepareDelete?: (payload: { repoRoot: string; branch: string }) => Promise<{
    branchMerged: boolean
    forceRequired: boolean
    warnings: string[]
  }>
  gitBranchDeleteWithPreflight?: (payload: {
    repoRoot: string
    branch: string
    forceIfUnmerged?: boolean
  }) => Promise<{
    branchDeleted: boolean
    forcedBranchDelete: boolean
    branchMerged: boolean
  }>
  gitPreparePush?: (payload: { repoRoot: string }) => Promise<
    | {
        hasUpstream: false
        confirmationMessage: string
      }
    | {
        hasUpstream: true
        branch: string
        remote: string
        commitCount: number
        confirmationMessage: string
      }
  >
  gitPullWithPreferences?: (payload: {
    repoRoot: string
  }) => Promise<{ summary: string; rebase: boolean }>
  gitCommitWorktree?: (payload: {
    repoRoot: string
    message: string
    stageAll?: boolean
  }) => Promise<{ hash: string; summary: string }>
  gitPushWorktree?: (payload: { repoRoot: string }) => Promise<{ branch: string; remote: string }>
  gitFetchWorktree?: (payload: { repoRoot: string }) => Promise<void>
  gitStashWorktree?: (payload: { repoRoot: string }) => Promise<void>
  gitStashPopWorktree?: (payload: { repoRoot: string }) => Promise<void>
  gitBranchCreateFromHead?: (payload: { repoRoot: string; branch: string }) => Promise<void>
  worktreeCreate?: (
    payload:
      | {
          repoRoot: string
          worktreePath: string
          mode: 'new'
          branch: string
          baseBranch: string
        }
      | {
          repoRoot: string
          worktreePath: string
          mode: 'existing'
          branch: string
          createLocalTracking?: boolean
        },
  ) => Promise<{ branch: string; worktreePath: string }>
  taskTrackerCreateWorktreeFromTask?: (payload: {
    connectionId: string
    task: Record<string, unknown>
    boardId?: string
    branchType?: string
    repoRoot: string
    worktreePath: string
    baseBranch: string
  }) => Promise<{ branchName: string; worktreePath: string }>
  taskTrackerBuildTaskContext?: (payload: {
    connectionId: string
    task: Record<string, unknown>
    repoRoot?: string
    trackerId?: string
  }) => Promise<string>
  tabUpdatePaneTitle?: (
    worktreePath: string,
    sessionId: string,
    title: string,
  ) => Promise<{ activeTabId: string | null }>
  tabUpdatePaneUrl?: (
    worktreePath: string,
    sessionId: string,
    url: string,
  ) => Promise<{ activeTabId: string | null }>
  tabUpdateTmuxSessionName?: (
    worktreePath: string,
    oldName: string,
    newName: string,
  ) => Promise<{ activeTabId: string | null }>
  tabHandlePtyExit?: (
    worktreePath: string,
    sessionId: string,
    exitCode: number,
    tmuxSessionName?: string,
  ) => Promise<{ activeTabId: string | null }>
  tabKillTmuxPane?: (
    worktreePath: string,
    tabId: string,
    paneId: string,
  ) => Promise<{ activeTabId: string | null }>
  tabReattachTmuxPane?: (
    worktreePath: string,
    tabId: string,
    paneId: string,
  ) => Promise<{ activeTabId: string | null }>
  tabToggleFocusedInspector?: (
    worktreePath: string,
    tabId: string,
  ) => Promise<{ activeTabId: string | null }>
  tabSplitPane?: (
    worktreePath: string,
    tabId: string,
    paneId: string,
    direction: 'horizontal' | 'vertical',
  ) => Promise<unknown>
  tabFocusPane?: (worktreePath: string, tabId: string, paneId: string) => Promise<unknown>
  tabClosePane?: (worktreePath: string, tabId: string, paneId: string) => Promise<unknown>
  tabCloseAllForWorktree?: (
    worktreePath: string,
  ) => Promise<{ tabs: unknown[]; activeTabId: string | null }>
  tabKillAll?: () => Promise<{
    tabsByWorktree: Record<string, unknown[]>
    activeTabIdByWorktree: Record<string, string | null>
  }>
  tabRestoreLayout?: (
    worktreePath: string,
    layoutJson: string,
  ) => Promise<{
    restored?: boolean
    tabs: Array<{
      id: string
      toolId: string
      suspended?: unknown
      rootSplit: {
        type: 'leaf' | 'split'
        pane?: {
          paneType?: string
          url?: string
          isRunning?: boolean
        }
      }
    }>
    activeTabId: string | null
  }>
  tabResumeSuspendedTab?: (
    worktreePath: string,
    tabId: string,
  ) => Promise<{ activeTabId: string | null }>
  tabSaveCurrentLayout?: (worktreePath: string) => Promise<unknown>
  tabFocusSession?: (sessionId: string) => Promise<{ activeTabId: string | null } | null>
  tabNavigatePaneFocus?: (
    worktreePath: string,
    tabId: string,
    direction: 'left' | 'right' | 'up' | 'down',
  ) => Promise<unknown>
  tabUpdateSplitRatio?: (
    worktreePath: string,
    tabId: string,
    splitId: string,
    ratio: number,
  ) => Promise<unknown>
  tabSetActiveTab?: (worktreePath: string, tabId: string) => Promise<unknown>
  tabMoveTab?: (worktreePath: string, fromIndex: number, toIndex: number) => Promise<unknown>
  tabMoveTabToSplit?: (
    worktreePath: string,
    sourceTabId: string,
    targetTabId: string,
    targetPaneId: string,
    direction: 'horizontal' | 'vertical',
    position: 'first' | 'second',
  ) => Promise<unknown>
  tabMovePaneToTarget?: (
    worktreePath: string,
    sourceTabId: string,
    sourcePaneId: string,
    targetTabId: string,
    targetPaneId: string,
    direction: 'horizontal' | 'vertical',
    position: 'first' | 'second',
  ) => Promise<unknown>
  tabDetachPaneToTab?: (
    worktreePath: string,
    sourceTabId: string,
    sourcePaneId: string,
  ) => Promise<{ openedTab?: { id: string } }>
}

function hasProject(snapshot: AppStateSnapshot, projectPath: string): boolean {
  return snapshot.workspace.projects.some(
    (project) => project.workspace.path === projectPath || project.repoRoot === projectPath,
  )
}

async function dismissSetupWizard(page: Page): Promise<void> {
  const skipSetup = page.getByRole('button', { name: 'Skip setup' })
  if (!(await skipSetup.isVisible({ timeout: 1_000 }).catch(() => false))) return
  await skipSetup.click()
  await page.getByRole('dialog', { name: 'Setup wizard' }).waitFor({
    state: 'detached',
    timeout: 5_000,
  })
}

async function openShellTabWithPane(
  page: Page,
  projectPath: string,
): Promise<{ id: string; paneId: string; sessionId: string; wsUrl: string }> {
  return page.evaluate(async (path) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.tabOpenTool !== 'function') {
      throw new Error('Missing tabOpenTool API')
    }
    const result = await api.tabOpenTool('shell', path)
    const tabId = result.openedTab?.id
    if (!tabId) throw new Error('Shell tab did not open')

    const snapshot = await api.getAppState()
    const tab = snapshot.tabs?.tabsByWorktree[path]?.find((candidate) => candidate.id === tabId)
    const pane = tab?.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
    if (!pane) throw new Error('Shell pane did not open')

    return { id: tabId, paneId: pane.id, sessionId: pane.sessionId, wsUrl: pane.wsUrl }
  }, projectPath)
}

test.beforeEach(async () => {
  extraTmpPaths = []
  tmpDir = await mkdtemp(join(homedir(), 'canopy-e2e-app-state-'))
  execSync('git init', { cwd: tmpDir })
  execSync('git config user.email "test@test.com"', { cwd: tmpDir })
  execSync('git config user.name "Test"', { cwd: tmpDir })
  await writeFile(join(tmpDir, 'README.md'), '# Test Project\n')
  execSync('git add . && git commit -m "init"', { cwd: tmpDir })
  await writeFile(join(tmpDir, 'DIRTY.txt'), 'uncommitted\n')
})

test.afterEach(async () => {
  await Promise.all([
    rm(tmpDir, { recursive: true, force: true }),
    ...extraTmpPaths.map((target) => rm(target, { recursive: true, force: true })),
  ])
})

test('main process exposes and publishes app state snapshots', async ({ electronApp, page }) => {
  await page.waitForFunction(() => !!(window as unknown as { api?: unknown }).api)
  await dismissSetupWizard(page)

  const apiShape = await page.evaluate(() => {
    const api = (window as unknown as { api: AppStateApi }).api
    return {
      getAppState: typeof api.getAppState,
      onAppStateChanged: typeof api.onAppStateChanged,
      tabSyncState: typeof (api as AppStateApi & { tabSyncState?: unknown }).tabSyncState,
      tabSaveLayout: typeof (api as AppStateApi & { tabSaveLayout?: unknown }).tabSaveLayout,
      hasChildProcess: typeof (api as AppStateApi & { hasChildProcess?: unknown }).hasChildProcess,
      writeFile: typeof (api as AppStateApi & { writeFile?: unknown }).writeFile,
      readFile: typeof (api as AppStateApi & { readFile?: unknown }).readFile,
      statFile: typeof (api as AppStateApi & { statFile?: unknown }).statFile,
      readDir: typeof (api as AppStateApi & { readDir?: unknown }).readDir,
      createFile: typeof (api as AppStateApi & { createFile?: unknown }).createFile,
      mkdir: typeof (api as AppStateApi & { mkdir?: unknown }).mkdir,
      confirmUnsavedChanges: typeof (api as AppStateApi & { confirmUnsavedChanges?: unknown })
        .confirmUnsavedChanges,
      fileTreeReadDir: typeof api.fileTreeReadDir,
      fileTreeCreateFile: typeof api.fileTreeCreateFile,
      fileTreeCreateDirectory: typeof api.fileTreeCreateDirectory,
      fileTreeGetGitStatus: typeof api.fileTreeGetGitStatus,
      changesGetDiff: typeof api.changesGetDiff,
      changesStageFile: typeof api.changesStageFile,
      changesRevertFile: typeof api.changesRevertFile,
      taskTrackerPrepareBranchFromTask: typeof api.taskTrackerPrepareBranchFromTask,
      taskTrackerCreateBranchFromTask: typeof api.taskTrackerCreateBranchFromTask,
      worktreeRemoveWithBranch: typeof api.worktreeRemoveWithBranch,
      worktreePrepareRemove: typeof api.worktreePrepareRemove,
      worktreeGetMergedBranches: typeof api.worktreeGetMergedBranches,
      worktreeListBranches: typeof api.worktreeListBranches,
      worktreeRefreshBranches: typeof api.worktreeRefreshBranches,
      gitBranchPrepareDelete: typeof api.gitBranchPrepareDelete,
      gitBranchDeleteWithPreflight: typeof api.gitBranchDeleteWithPreflight,
      gitPreparePush: typeof api.gitPreparePush,
      gitPullWithPreferences: typeof api.gitPullWithPreferences,
      gitCommitWorktree: typeof api.gitCommitWorktree,
      gitPushWorktree: typeof api.gitPushWorktree,
      gitFetchWorktree: typeof api.gitFetchWorktree,
      gitStashWorktree: typeof api.gitStashWorktree,
      gitStashPopWorktree: typeof api.gitStashPopWorktree,
      gitBranchCreateFromHead: typeof api.gitBranchCreateFromHead,
      worktreeCreate: typeof api.worktreeCreate,
      taskTrackerCreateWorktreeFromTask: typeof api.taskTrackerCreateWorktreeFromTask,
      taskTrackerBuildTaskContext: typeof api.taskTrackerBuildTaskContext,
      tabOpenTool: typeof api.tabOpenTool,
      tabSaveCurrentLayout: typeof api.tabSaveCurrentLayout,
      tabPrepareCloseTab: typeof api.tabPrepareCloseTab,
      tabPrepareCloseAllForWorktree: typeof api.tabPrepareCloseAllForWorktree,
      tabGetCloseWarning: typeof api.tabGetCloseWarning,
      tabSaveEditorFile: typeof api.tabSaveEditorFile,
      tabPrepareCloseEditorFile: typeof api.tabPrepareCloseEditorFile,
      tabLoadEditorFile: typeof api.tabLoadEditorFile,
      tabReopenClosedTab: typeof api.tabReopenClosedTab,
    }
  })

  expect(apiShape.getAppState).toBe('function')
  expect(apiShape.onAppStateChanged).toBe('function')
  expect(apiShape.tabSyncState).toBe('undefined')
  expect(apiShape.tabSaveLayout).toBe('undefined')
  expect(apiShape.hasChildProcess).toBe('undefined')
  expect(apiShape.writeFile).toBe('undefined')
  expect(apiShape.readFile).toBe('undefined')
  expect(apiShape.statFile).toBe('undefined')
  expect(apiShape.readDir).toBe('undefined')
  expect(apiShape.createFile).toBe('undefined')
  expect(apiShape.mkdir).toBe('undefined')
  expect(apiShape.confirmUnsavedChanges).toBe('undefined')
  expect(apiShape.fileTreeReadDir).toBe('function')
  expect(apiShape.fileTreeCreateFile).toBe('function')
  expect(apiShape.fileTreeCreateDirectory).toBe('function')
  expect(apiShape.fileTreeGetGitStatus).toBe('function')
  expect(apiShape.changesGetDiff).toBe('function')
  expect(apiShape.changesStageFile).toBe('function')
  expect(apiShape.changesRevertFile).toBe('function')
  expect(apiShape.taskTrackerPrepareBranchFromTask).toBe('function')
  expect(apiShape.taskTrackerCreateBranchFromTask).toBe('function')
  expect(apiShape.worktreeRemoveWithBranch).toBe('function')
  expect(apiShape.worktreePrepareRemove).toBe('function')
  expect(apiShape.worktreeGetMergedBranches).toBe('function')
  expect(apiShape.worktreeListBranches).toBe('function')
  expect(apiShape.worktreeRefreshBranches).toBe('function')
  expect(apiShape.gitBranchPrepareDelete).toBe('function')
  expect(apiShape.gitBranchDeleteWithPreflight).toBe('function')
  expect(apiShape.gitPreparePush).toBe('function')
  expect(apiShape.gitPullWithPreferences).toBe('function')
  expect(apiShape.gitCommitWorktree).toBe('function')
  expect(apiShape.gitPushWorktree).toBe('function')
  expect(apiShape.gitFetchWorktree).toBe('function')
  expect(apiShape.gitStashWorktree).toBe('function')
  expect(apiShape.gitStashPopWorktree).toBe('function')
  expect(apiShape.gitBranchCreateFromHead).toBe('function')
  expect(apiShape.worktreeCreate).toBe('function')
  expect(apiShape.taskTrackerCreateWorktreeFromTask).toBe('function')
  expect(apiShape.taskTrackerBuildTaskContext).toBe('function')
  expect(apiShape.tabOpenTool).toBe('function')
  expect(apiShape.tabSaveCurrentLayout).toBe('function')
  expect(apiShape.tabPrepareCloseTab).toBe('function')
  expect(apiShape.tabPrepareCloseAllForWorktree).toBe('function')
  expect(apiShape.tabGetCloseWarning).toBe('function')
  expect(apiShape.tabSaveEditorFile).toBe('function')
  expect(apiShape.tabPrepareCloseEditorFile).toBe('function')
  expect(apiShape.tabLoadEditorFile).toBe('function')
  expect(apiShape.tabReopenClosedTab).toBe('function')

  const newWindowPromise = electronApp.waitForEvent('window')
  await electronApp.evaluate(({ app, dialog }, projectPath) => {
    const originalShowMessageBox = dialog.showMessageBox
    dialog.showMessageBox = async () => ({ response: 0, checkboxChecked: false })
    app.emit(
      'open-url',
      { preventDefault: () => {} },
      `canopy://open?path=${encodeURIComponent(projectPath)}`,
    )
    setTimeout(() => {
      dialog.showMessageBox = originalShowMessageBox
    }, 1_000)
  }, tmpDir)

  const workspacePage = await newWindowPromise
  await workspacePage.waitForFunction(() => {
    const api = (window as unknown as { api?: AppStateApi }).api
    return typeof api?.getAppState === 'function'
  })
  await dismissSetupWizard(workspacePage)

  await expect
    .poll(
      () =>
        workspacePage.evaluate((projectPath) => {
          const api = (window as unknown as { api: Required<AppStateApi> }).api
          return api
            .getAppState()
            .then((snapshot) =>
              snapshot.workspace.projects.some(
                (project) =>
                  project.workspace.path === projectPath || project.repoRoot === projectPath,
              ),
            )
        }, tmpDir),
      { timeout: 10_000 },
    )
    .toBe(true)

  await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.tabOpenTool !== 'function') {
      throw new Error('Missing tabOpenTool API')
    }
    return api.tabOpenTool('shell', projectPath)
  }, tmpDir)

  await expect
    .poll(
      () =>
        workspacePage.evaluate((projectPath) => {
          const api = (window as unknown as { api: Required<AppStateApi> }).api
          return api.getAppState().then((snapshot) => {
            const tabs = snapshot.tabs?.tabsByWorktree[projectPath] ?? []
            const activeTabId = snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null
            return tabs.some((tab) => tab.toolId === 'shell' && tab.id === activeTabId)
          })
        }, tmpDir),
      { timeout: 10_000 },
    )
    .toBe(true)

  const shellTab = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    return api.getAppState().then((snapshot) => {
      const tabs = snapshot.tabs?.tabsByWorktree[projectPath] ?? []
      const activeTabId = snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null
      return tabs.find((tab) => tab.toolId === 'shell' && tab.id === activeTabId) ?? null
    })
  }, tmpDir)
  expect(shellTab).not.toBeNull()
  await expect(workspacePage.getByRole('tablist', { name: 'Terminal tabs' })).toBeVisible({
    timeout: 10_000,
  })
  await expect(workspacePage.getByText(/Press .* to open a new tab/)).not.toBeVisible()

  const modifier = process.platform === 'darwin' ? 'Meta' : 'Control'
  await workspacePage.keyboard.press(`${modifier}+K`)
  await expect(workspacePage.getByRole('dialog', { name: 'Command palette' })).toBeVisible()
  await workspacePage.keyboard.press('Escape')
  await expect(workspacePage.getByRole('dialog', { name: 'Command palette' })).not.toBeVisible()
  await workspacePage.keyboard.press(`${modifier}+,`)
  await expect(workspacePage.getByRole('dialog', { name: /Settings/ })).toBeVisible()
  await workspacePage.keyboard.press('Escape')
  await expect(workspacePage.getByRole('dialog', { name: /Settings/ })).not.toBeVisible()

  const initialPaneId =
    shellTab?.rootSplit.type === 'leaf' ? shellTab.rootSplit.pane?.id : undefined
  const initialSessionId =
    shellTab?.rootSplit.type === 'leaf' ? shellTab.rootSplit.pane?.sessionId : undefined
  expect(initialPaneId).toBeTruthy()
  expect(initialSessionId).toBeTruthy()

  await expect(
    page.evaluate(async (sessionId) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      await api.resizePty(sessionId, 100, 30)
      return true
    }, initialSessionId!),
  ).resolves.toBe(true)

  const closeWarning = await workspacePage.evaluate(
    ({ projectPath, tabId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabGetCloseWarning !== 'function') {
        throw new Error('Missing tabGetCloseWarning API')
      }
      return api.tabGetCloseWarning(projectPath, { kind: 'tab', tabId })
    },
    { projectPath: tmpDir, tabId: shellTab!.id },
  )
  expect(closeWarning).toEqual({ description: null })

  const closePreflight = await workspacePage.evaluate(
    ({ projectPath, tabId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabPrepareCloseTab !== 'function') {
        throw new Error('Missing tabPrepareCloseTab API')
      }
      return api.tabPrepareCloseTab(projectPath, tabId)
    },
    { projectPath: tmpDir, tabId: shellTab!.id },
  )
  expect(closePreflight).toEqual({ ok: true })

  const rootEntries = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.fileTreeReadDir !== 'function') {
      throw new Error('Missing fileTreeReadDir API')
    }
    return api.fileTreeReadDir(projectPath)
  }, tmpDir)
  expect(rootEntries).toEqual(
    expect.arrayContaining([
      expect.objectContaining({ name: 'README.md', isDirectory: false, size: 15 }),
      expect.objectContaining({ name: 'DIRTY.txt', isDirectory: false, size: 12 }),
    ]),
  )

  const newFilePath = join(tmpDir, 'nested', 'from-file-tree.txt')
  const newDirPath = join(tmpDir, 'created-dir')
  await workspacePage.evaluate(
    ({ filePath, dirPath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.fileTreeCreateFile !== 'function') {
        throw new Error('Missing fileTreeCreateFile API')
      }
      if (typeof api.fileTreeCreateDirectory !== 'function') {
        throw new Error('Missing fileTreeCreateDirectory API')
      }
      return Promise.all([api.fileTreeCreateFile(filePath), api.fileTreeCreateDirectory(dirPath)])
    },
    { filePath: newFilePath, dirPath: newDirPath },
  )
  expect(await readFile(newFilePath, 'utf8')).toBe('')
  expect((await stat(newDirPath)).isDirectory()).toBe(true)

  const fileTreeGitStatus = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.fileTreeGetGitStatus !== 'function') {
      throw new Error('Missing fileTreeGetGitStatus API')
    }
    return api.fileTreeGetGitStatus(projectPath, projectPath)
  }, tmpDir)
  expect(fileTreeGitStatus.statuses).toMatchObject({
    'DIRTY.txt': '?',
    'nested/': '?',
  })
  expect(fileTreeGitStatus.changedDirs).toEqual(['nested'])
  expect(fileTreeGitStatus.affectedPaths).toEqual(expect.arrayContaining(['DIRTY.txt', 'nested/']))

  await writeFile(join(tmpDir, 'README.md'), '# Test Project\ntracked change\n')
  const taskTrackerBranch = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.taskTrackerPrepareBranchFromTask !== 'function') {
      throw new Error('Missing taskTrackerPrepareBranchFromTask API')
    }
    if (typeof api.taskTrackerCreateBranchFromTask !== 'function') {
      throw new Error('Missing taskTrackerCreateBranchFromTask API')
    }
    const task = {
      key: 'CANOPY-101',
      summary: 'Move renderer branch creation to main',
      description: '',
      status: 'Open',
      priority: 'Medium',
      type: 'task',
    }
    return api
      .taskTrackerPrepareBranchFromTask({
        connectionId: 'e2e',
        task,
        repoRoot: projectPath,
      })
      .then((prepared) =>
        api
          .taskTrackerCreateBranchFromTask({
            connectionId: 'e2e',
            task,
            repoRoot: projectPath,
            baseBranch: 'HEAD',
            stashBeforeCreate: true,
          })
          .then((created) => ({ prepared, created })),
      )
  }, tmpDir)
  expect(taskTrackerBranch).toEqual({
    prepared: { branchName: 'CANOPY-101' },
    created: { branchName: 'CANOPY-101' },
  })
  expect(execSync('git branch --list CANOPY-101', { cwd: tmpDir }).toString()).toContain(
    'CANOPY-101',
  )
  expect(await readFile(join(tmpDir, 'README.md'), 'utf8')).toBe('# Test Project\n')

  const taskWorktreePath = `${tmpDir}-task-worktree`
  extraTmpPaths.push(taskWorktreePath)
  const taskWorktreeResult = await workspacePage.evaluate(
    ({ projectPath, worktreePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.taskTrackerCreateWorktreeFromTask !== 'function') {
        throw new Error('Missing taskTrackerCreateWorktreeFromTask API')
      }
      return api.taskTrackerCreateWorktreeFromTask({
        connectionId: 'e2e',
        task: {
          key: 'CANOPY-202',
          summary: 'Create worktree from task in main',
          description: '',
          status: 'Open',
          priority: 'Medium',
          type: 'task',
        },
        repoRoot: projectPath,
        worktreePath,
        baseBranch: 'HEAD',
      })
    },
    { projectPath: tmpDir, worktreePath: taskWorktreePath },
  )
  expect(taskWorktreeResult).toEqual({
    branchName: 'CANOPY-202',
    worktreePath: taskWorktreePath,
  })
  expect((await stat(join(taskWorktreePath, 'README.md'))).isFile()).toBe(true)
  expect(execSync('git branch --list CANOPY-202', { cwd: tmpDir }).toString()).toContain(
    'CANOPY-202',
  )

  const genericWorktreePath = `${tmpDir}-generic-worktree`
  extraTmpPaths.push(genericWorktreePath)
  const genericWorktreeResult = await workspacePage.evaluate(
    ({ projectPath, worktreePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.worktreeCreate !== 'function') {
        throw new Error('Missing worktreeCreate API')
      }
      return api.worktreeCreate({
        repoRoot: projectPath,
        worktreePath,
        mode: 'new',
        branch: 'CANOPY-GENERIC',
        baseBranch: 'HEAD',
      })
    },
    { projectPath: tmpDir, worktreePath: genericWorktreePath },
  )
  expect(genericWorktreeResult).toEqual({
    branch: 'CANOPY-GENERIC',
    worktreePath: genericWorktreePath,
  })
  expect((await stat(join(genericWorktreePath, 'README.md'))).isFile()).toBe(true)

  const taskContext = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.taskTrackerBuildTaskContext !== 'function') {
      throw new Error('Missing taskTrackerBuildTaskContext API')
    }
    return api.taskTrackerBuildTaskContext({
      connectionId: 'e2e',
      repoRoot: projectPath,
      task: {
        key: 'CANOPY-303',
        summary: 'Build task context in main',
        description: 'First line\r\n\r\n\r\nSecond line',
        status: 'Open',
        priority: 'High',
        type: 'task',
        url: 'https://example.test/CANOPY-303',
      },
    })
  }, tmpDir)
  expect(taskContext).toContain('Work on the following task:')
  expect(taskContext).toContain('# CANOPY-303: Build task context in main')
  expect(taskContext).toContain('Status: Open | Priority: High | Type: task')
  expect(taskContext).toContain('URL: https://example.test/CANOPY-303')
  expect(taskContext).toContain('First line\n\nSecond line')
  expect(taskContext).not.toContain('undefined')

  const pushPreflight = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.gitPreparePush !== 'function') {
      throw new Error('Missing gitPreparePush API')
    }
    return api.gitPreparePush({ repoRoot: projectPath })
  }, tmpDir)
  expect(pushPreflight).toEqual({
    hasUpstream: false,
    confirmationMessage: 'No upstream branch — push and set tracking to origin?',
  })

  await writeFile(join(tmpDir, '.git', 'info', 'exclude'), '\n.canopy-e2e-pull/\n', {
    flag: 'a',
  })
  const pullBasePath = join(tmpDir, '.canopy-e2e-pull')
  const pullRepoPath = join(pullBasePath, 'repo')
  const pullRemotePath = join(pullBasePath, 'remote.git')
  const pullWriterPath = join(pullBasePath, 'writer')
  execFileSync('git', ['init', pullRepoPath])
  execSync('git config user.email "test@test.com"', { cwd: pullRepoPath })
  execSync('git config user.name "Test"', { cwd: pullRepoPath })
  await writeFile(join(pullRepoPath, 'README.md'), '# Pull Repo\n')
  execSync('git add . && git commit -m "init"', { cwd: pullRepoPath })
  const pullBranch = execFileSync('git', ['rev-parse', '--abbrev-ref', 'HEAD'], {
    cwd: pullRepoPath,
  })
    .toString()
    .trim()
  execFileSync('git', ['clone', '--bare', pullRepoPath, pullRemotePath])
  execFileSync('git', ['remote', 'add', 'origin', pullRemotePath], { cwd: pullRepoPath })
  execFileSync('git', ['push', '-u', 'origin', pullBranch], { cwd: pullRepoPath })
  execFileSync('git', ['clone', '-b', pullBranch, pullRemotePath, pullWriterPath])
  execSync('git config user.email "test@test.com"', { cwd: pullWriterPath })
  execSync('git config user.name "Test"', { cwd: pullWriterPath })
  await writeFile(join(pullWriterPath, 'PULL_REMOTE.txt'), 'remote update\n')
  execSync('git add PULL_REMOTE.txt && git commit -m "remote update"', { cwd: pullWriterPath })
  execFileSync('git', ['push', 'origin', pullBranch], { cwd: pullWriterPath })

  const pullResult = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.setPref !== 'function') {
      throw new Error('Missing setPref API')
    }
    if (typeof api.gitPullWithPreferences !== 'function') {
      throw new Error('Missing gitPullWithPreferences API')
    }
    return api
      .setPref('gitPullRebase', 'false')
      .then(() => api.gitPullWithPreferences({ repoRoot: projectPath }))
  }, pullRepoPath)
  expect(pullResult).toEqual({ summary: '1 file(s) updated', rebase: false })
  expect(await readFile(join(pullRepoPath, 'PULL_REMOTE.txt'), 'utf8')).toBe('remote update\n')
  await writeFile(join(pullWriterPath, 'FETCH_REMOTE.txt'), 'fetch update\n')
  execSync('git add FETCH_REMOTE.txt && git commit -m "fetch update"', { cwd: pullWriterPath })
  execFileSync('git', ['push', 'origin', pullBranch], { cwd: pullWriterPath })

  await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.gitFetchWorktree !== 'function') {
      throw new Error('Missing gitFetchWorktree API')
    }
    return api.gitFetchWorktree({ repoRoot: projectPath })
  }, pullRepoPath)
  const fetchedRemoteHead = execFileSync(
    'git',
    ['rev-parse', `origin/${pullBranch}:FETCH_REMOTE.txt`],
    { cwd: pullRepoPath },
  )
    .toString()
    .trim()
  expect(fetchedRemoteHead).toBeTruthy()

  await writeFile(join(pullRepoPath, 'README.md'), '# Pull Repo\nlocal change\n')
  await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.gitStashWorktree !== 'function') {
      throw new Error('Missing gitStashWorktree API')
    }
    return api.gitStashWorktree({ repoRoot: projectPath })
  }, pullRepoPath)
  expect(execSync('git status --porcelain', { cwd: pullRepoPath }).toString().trim()).toBe('')
  expect(execSync('git stash list', { cwd: pullRepoPath }).toString()).toContain('WIP on')
  await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.gitStashPopWorktree !== 'function') {
      throw new Error('Missing gitStashPopWorktree API')
    }
    return api.gitStashPopWorktree({ repoRoot: projectPath })
  }, pullRepoPath)
  expect(await readFile(join(pullRepoPath, 'README.md'), 'utf8')).toBe(
    '# Pull Repo\nlocal change\n',
  )
  const commitResult = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.gitCommitWorktree !== 'function') {
      throw new Error('Missing gitCommitWorktree API')
    }
    return api.gitCommitWorktree({
      repoRoot: projectPath,
      message: 'local wrapper commit',
      stageAll: true,
    })
  }, pullRepoPath)
  expect(commitResult.hash).toBeTruthy()
  expect(execSync('git log -1 --pretty=%s', { cwd: pullRepoPath }).toString().trim()).toBe(
    'local wrapper commit',
  )
  await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.gitBranchCreateFromHead !== 'function') {
      throw new Error('Missing gitBranchCreateFromHead API')
    }
    return api.gitBranchCreateFromHead({
      repoRoot: projectPath,
      branch: 'CANOPY-WRAPPER-BRANCH',
    })
  }, pullRepoPath)
  expect(
    execSync('git branch --list CANOPY-WRAPPER-BRANCH', { cwd: pullRepoPath }).toString(),
  ).toContain('CANOPY-WRAPPER-BRANCH')
  const listedBranches = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.worktreeListBranches !== 'function') {
      throw new Error('Missing worktreeListBranches API')
    }
    return api.worktreeListBranches({ repoRoot: projectPath })
  }, pullRepoPath)
  expect(listedBranches.local).toContain('CANOPY-WRAPPER-BRANCH')
  execFileSync('git', ['checkout', '-b', 'CANOPY-REFRESH-BRANCH'], { cwd: pullWriterPath })
  await writeFile(join(pullWriterPath, 'REFRESH_BRANCH.txt'), 'refresh branch\n')
  execSync('git add REFRESH_BRANCH.txt && git commit -m "refresh branch"', {
    cwd: pullWriterPath,
  })
  execFileSync('git', ['push', 'origin', 'CANOPY-REFRESH-BRANCH'], { cwd: pullWriterPath })
  const refreshedBranches = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.worktreeRefreshBranches !== 'function') {
      throw new Error('Missing worktreeRefreshBranches API')
    }
    return api.worktreeRefreshBranches({ repoRoot: projectPath })
  }, pullRepoPath)
  expect(refreshedBranches.remote).toContain('origin/CANOPY-REFRESH-BRANCH')
  await writeFile(join(pullRepoPath, 'README.md'), '# Pull Repo\nchanges diff\n')
  const changesDiff = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.changesGetDiff !== 'function') {
      throw new Error('Missing changesGetDiff API')
    }
    return api.changesGetDiff({ worktreePath: projectPath })
  }, pullRepoPath)
  expect(changesDiff.files.map((file) => file.path)).toContain('README.md')
  await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.changesRevertFile !== 'function') {
      throw new Error('Missing changesRevertFile API')
    }
    return api.changesRevertFile({ worktreePath: projectPath, filePath: 'README.md' })
  }, pullRepoPath)
  expect(execSync('git status --porcelain', { cwd: pullRepoPath }).toString().trim()).toBe('')
  await writeFile(join(pullRepoPath, 'README.md'), '# Pull Repo\nchanges staged\n')
  await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.changesStageFile !== 'function') {
      throw new Error('Missing changesStageFile API')
    }
    return api.changesStageFile({ worktreePath: projectPath, filePath: 'README.md' })
  }, pullRepoPath)
  expect(execSync('git diff --cached --name-only', { cwd: pullRepoPath }).toString().trim()).toBe(
    'README.md',
  )

  const pushRepoPath = join(pullBasePath, 'push-repo')
  const pushRemotePath = join(pullBasePath, 'push-remote.git')
  execFileSync('git', ['init', pushRepoPath])
  execSync('git config user.email "test@test.com"', { cwd: pushRepoPath })
  execSync('git config user.name "Test"', { cwd: pushRepoPath })
  await writeFile(join(pushRepoPath, 'README.md'), '# Push Repo\n')
  execSync('git add . && git commit -m "init"', { cwd: pushRepoPath })
  execFileSync('git', ['init', '--bare', pushRemotePath])
  execFileSync('git', ['remote', 'add', 'origin', pushRemotePath], { cwd: pushRepoPath })
  const pushBranch = execFileSync('git', ['rev-parse', '--abbrev-ref', 'HEAD'], {
    cwd: pushRepoPath,
  })
    .toString()
    .trim()
  const pushHead = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: pushRepoPath })
    .toString()
    .trim()
  await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.gitPushWorktree !== 'function') {
      throw new Error('Missing gitPushWorktree API')
    }
    return api.gitPushWorktree({ repoRoot: projectPath })
  }, pushRepoPath)
  const remotePushHead = execFileSync('git', ['rev-parse', pushBranch], {
    cwd: pushRemotePath,
  })
    .toString()
    .trim()
  expect(remotePushHead).toBe(pushHead)

  execSync('git branch CANOPY-BATCH-MERGED HEAD', { cwd: tmpDir })
  const batchHead = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: tmpDir }).toString().trim()
  const batchTree = execFileSync('git', ['rev-parse', `${batchHead}^{tree}`], { cwd: tmpDir })
    .toString()
    .trim()
  const batchUnmergedCommit = execFileSync(
    'git',
    ['commit-tree', batchTree, '-p', batchHead, '-m', 'batch unmerged branch e2e'],
    { cwd: tmpDir },
  )
    .toString()
    .trim()
  execFileSync('git', ['branch', 'CANOPY-BATCH-UNMERGED', batchUnmergedCommit], { cwd: tmpDir })
  const mergedBranchBatch = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.worktreeGetMergedBranches !== 'function') {
      throw new Error('Missing worktreeGetMergedBranches API')
    }
    return api.worktreeGetMergedBranches({
      repoRoot: projectPath,
      branches: ['CANOPY-BATCH-MERGED', 'CANOPY-BATCH-UNMERGED', '(detached)'],
    })
  }, tmpDir)
  expect(mergedBranchBatch).toEqual({ mergedBranches: ['CANOPY-BATCH-MERGED'] })

  execSync('git branch CANOPY-BRANCH-MERGED HEAD', { cwd: tmpDir })
  const mergedBranchDelete = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.gitBranchPrepareDelete !== 'function') {
      throw new Error('Missing gitBranchPrepareDelete API')
    }
    if (typeof api.gitBranchDeleteWithPreflight !== 'function') {
      throw new Error('Missing gitBranchDeleteWithPreflight API')
    }
    return api
      .gitBranchPrepareDelete({
        repoRoot: projectPath,
        branch: 'CANOPY-BRANCH-MERGED',
      })
      .then((preflight) =>
        api
          .gitBranchDeleteWithPreflight({
            repoRoot: projectPath,
            branch: 'CANOPY-BRANCH-MERGED',
          })
          .then((deleted) => ({ preflight, deleted })),
      )
  }, tmpDir)
  expect(mergedBranchDelete).toEqual({
    preflight: { branchMerged: true, forceRequired: false, warnings: [] },
    deleted: { branchDeleted: true, forcedBranchDelete: false, branchMerged: true },
  })
  expect(
    execSync('git branch --list CANOPY-BRANCH-MERGED', { cwd: tmpDir }).toString().trim(),
  ).toBe('')

  const head = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: tmpDir }).toString().trim()
  const tree = execFileSync('git', ['rev-parse', `${head}^{tree}`], { cwd: tmpDir })
    .toString()
    .trim()
  const unmergedCommit = execFileSync(
    'git',
    ['commit-tree', tree, '-p', head, '-m', 'unmerged branch delete e2e'],
    { cwd: tmpDir },
  )
    .toString()
    .trim()
  execFileSync('git', ['branch', 'CANOPY-BRANCH-UNMERGED', unmergedCommit], { cwd: tmpDir })
  const unmergedBranchDelete = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.gitBranchPrepareDelete !== 'function') {
      throw new Error('Missing gitBranchPrepareDelete API')
    }
    if (typeof api.gitBranchDeleteWithPreflight !== 'function') {
      throw new Error('Missing gitBranchDeleteWithPreflight API')
    }
    return api
      .gitBranchPrepareDelete({
        repoRoot: projectPath,
        branch: 'CANOPY-BRANCH-UNMERGED',
      })
      .then((preflight) =>
        api
          .gitBranchDeleteWithPreflight({
            repoRoot: projectPath,
            branch: 'CANOPY-BRANCH-UNMERGED',
            forceIfUnmerged: true,
          })
          .then((deleted) => ({ preflight, deleted })),
      )
  }, tmpDir)
  expect(unmergedBranchDelete).toEqual({
    preflight: {
      branchMerged: false,
      forceRequired: true,
      warnings: ['Branch has not been fully merged.'],
    },
    deleted: { branchDeleted: true, forcedBranchDelete: true, branchMerged: false },
  })
  expect(
    execSync('git branch --list CANOPY-BRANCH-UNMERGED', { cwd: tmpDir }).toString().trim(),
  ).toBe('')

  const removeWorktreePath = `${tmpDir}-remove-worktree`
  extraTmpPaths.push(removeWorktreePath)
  execSync('git branch CANOPY-REMOVE HEAD', { cwd: tmpDir })
  execFileSync('git', ['worktree', 'add', removeWorktreePath, 'CANOPY-REMOVE'], { cwd: tmpDir })
  await writeFile(join(removeWorktreePath, 'feature.txt'), 'new branch work\n')
  execSync('git add feature.txt && git commit -m "worktree branch change"', {
    cwd: removeWorktreePath,
  })
  const worktreeRemovePreflight = await workspacePage.evaluate(
    ({ projectPath, worktreePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.worktreePrepareRemove !== 'function') {
        throw new Error('Missing worktreePrepareRemove API')
      }
      return api.worktreePrepareRemove({
        repoRoot: projectPath,
        worktreePath,
        branch: 'CANOPY-REMOVE',
      })
    },
    { projectPath: tmpDir, worktreePath: removeWorktreePath },
  )
  expect(worktreeRemovePreflight).toEqual({
    hasUncommittedChanges: false,
    unmergedCommitCount: 2,
    branchMerged: false,
    forceRequired: true,
    canDeleteBranch: false,
    warnings: ['2 unmerged commit(s) not on any remote.'],
  })
  const worktreeRemoveResult = await workspacePage.evaluate(
    ({ projectPath, worktreePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.worktreeRemoveWithBranch !== 'function') {
        throw new Error('Missing worktreeRemoveWithBranch API')
      }
      return api.worktreeRemoveWithBranch({
        repoRoot: projectPath,
        worktreePath,
        branch: 'CANOPY-REMOVE',
        deleteBranch: true,
        forceOnFailure: true,
      })
    },
    { projectPath: tmpDir, worktreePath: removeWorktreePath },
  )
  expect(worktreeRemoveResult).toEqual({
    worktreeRemoved: true,
    branchDeleted: true,
    forcedWorktreeRemove: false,
    forcedBranchDelete: true,
    leftoverPath: null,
  })
  await expect(
    stat(removeWorktreePath).then(
      () => true,
      () => false,
    ),
  ).resolves.toBe(false)
  expect(execSync('git branch --list CANOPY-REMOVE', { cwd: tmpDir }).toString().trim()).toBe('')

  // Partial-success path: a ghost worktree (unregistered by a previous failed
  // attempt, directory left behind) whose debris contains a file another process
  // holds open without delete sharing. Removal must still succeed and report the
  // surviving directory via leftoverPath instead of throwing. Windows-only: the
  // no-share file lock has no POSIX equivalent.
  if (process.platform === 'win32') {
    const ghostWorktreePath = `${tmpDir}-ghost-worktree`
    extraTmpPaths.push(ghostWorktreePath)
    execSync('git branch CANOPY-GHOST HEAD', { cwd: tmpDir })
    execFileSync('git', ['worktree', 'add', ghostWorktreePath, 'CANOPY-GHOST'], { cwd: tmpDir })
    // Turn it into the ghost state observed in the field: the .git link deleted by
    // an earlier failed removal, directory (with content) left on disk, and the —
    // now prunable — registration still present so the removal path validates.
    await rm(join(ghostWorktreePath, '.git'), { force: true })

    const lockedFile = join(ghostWorktreePath, 'feature.txt')
    const locker = spawn(
      'powershell.exe',
      [
        '-NoProfile',
        '-Command',
        `$f=[System.IO.File]::Open('${lockedFile.replace(/\\/g, '\\\\')}','Open','Read','None'); Start-Sleep -Seconds 60`,
      ],
      { stdio: 'ignore' },
    )
    try {
      // Give PowerShell a moment to acquire the handle.
      await new Promise((resolve) => setTimeout(resolve, 3000))
      const ghostRemoveResult = await workspacePage.evaluate(
        ({ projectPath, worktreePath }) => {
          const api = (window as unknown as { api: Required<AppStateApi> }).api
          return api.worktreeRemoveWithBranch({
            repoRoot: projectPath,
            worktreePath,
            branch: 'CANOPY-GHOST',
            deleteBranch: true,
            forceOnFailure: true,
          })
        },
        { projectPath: tmpDir, worktreePath: ghostWorktreePath },
      )
      expect(ghostRemoveResult.worktreeRemoved).toBe(true)
      expect(ghostRemoveResult.branchDeleted).toBe(true)
      expect(ghostRemoveResult.leftoverPath).not.toBeNull()
      await expect(
        stat(ghostWorktreePath).then(
          () => true,
          () => false,
        ),
      ).resolves.toBe(true)
    } finally {
      locker.kill()
    }
    expect(execSync('git branch --list CANOPY-GHOST', { cwd: tmpDir }).toString().trim()).toBe('')
  }

  // Submodule force-requirement: git refuses to remove a worktree containing an
  // initialized submodule even when clean — preflight must surface it as
  // forceRequired so the consent dialog runs BEFORE tab teardown, and the consented
  // removal must retry with --force.
  const submoduleSourcePath = `${tmpDir}-submodule-source`
  const submoduleWorktreePath = `${tmpDir}-submodule-worktree`
  extraTmpPaths.push(submoduleSourcePath, submoduleWorktreePath)
  execFileSync('git', ['init', submoduleSourcePath])
  execSync('git commit --allow-empty -m init', { cwd: submoduleSourcePath })
  execSync('git branch CANOPY-SUBMODULE HEAD', { cwd: tmpDir })
  execFileSync('git', ['worktree', 'add', submoduleWorktreePath, 'CANOPY-SUBMODULE'], {
    cwd: tmpDir,
  })
  execFileSync(
    'git',
    ['-c', 'protocol.file.allow=always', 'submodule', 'add', submoduleSourcePath, 'sub'],
    { cwd: submoduleWorktreePath },
  )
  execSync('git commit -m "add submodule"', { cwd: submoduleWorktreePath })

  const submodulePreflight = await workspacePage.evaluate(
    ({ projectPath, worktreePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      return api.worktreePrepareRemove({
        repoRoot: projectPath,
        worktreePath,
        branch: 'CANOPY-SUBMODULE',
      })
    },
    { projectPath: tmpDir, worktreePath: submoduleWorktreePath },
  )
  expect(submodulePreflight.forceRequired).toBe(true)
  expect(submodulePreflight.warnings).toContain(
    'Contains git submodules — git requires a forced removal.',
  )

  const submoduleRemoveResult = await workspacePage.evaluate(
    ({ projectPath, worktreePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      return api.worktreeRemoveWithBranch({
        repoRoot: projectPath,
        worktreePath,
        branch: 'CANOPY-SUBMODULE',
        deleteBranch: true,
        forceOnFailure: true,
      })
    },
    { projectPath: tmpDir, worktreePath: submoduleWorktreePath },
  )
  expect(submoduleRemoveResult.worktreeRemoved).toBe(true)
  expect(submoduleRemoveResult.forcedWorktreeRemove).toBe(true)
  expect(submoduleRemoveResult.branchDeleted).toBe(true)
  await expect(
    stat(submoduleWorktreePath).then(
      () => true,
      () => false,
    ),
  ).resolves.toBe(false)

  const injectedFocusResult = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.tabFocusSession !== 'function') {
      throw new Error('Missing tabFocusSession API')
    }
    return (
      api.tabFocusSession as unknown as (
        sessionId: string,
        options?: Record<string, unknown>,
      ) => Promise<unknown>
    )('renderer-injected-session', {
      tabsByWorktree: {
        [projectPath]: [
          {
            id: 'renderer-injected-tab',
            toolId: 'shell',
            toolName: 'Shell',
            name: 'Injected',
            worktreePath: projectPath,
            focusedPaneId: 'renderer-injected-pane',
            rootSplit: {
              type: 'leaf',
              pane: {
                id: 'renderer-injected-pane',
                sessionId: 'renderer-injected-session',
                wsUrl: '',
                toolId: 'shell',
                toolName: 'Shell',
                isRunning: true,
                exitCode: null,
                title: null,
              },
            },
          },
        ],
      },
    })
  }, tmpDir)
  expect(injectedFocusResult).toBeNull()

  await workspacePage.evaluate(
    ({ projectPath, sessionId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabUpdatePaneTitle !== 'function') {
        throw new Error('Missing tabUpdatePaneTitle API')
      }
      return api.tabUpdatePaneTitle(projectPath, sessionId, 'Main owned title')
    },
    { projectPath: tmpDir, sessionId: initialSessionId! },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
                (candidate) => candidate.id === tabId,
              )
              const pane = tab?.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
              return {
                activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
                title: pane?.title ?? null,
              }
            })
          },
          { projectPath: tmpDir, tabId: shellTab!.id },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      activeTabId: shellTab!.id,
      title: 'Main owned title',
    })

  await workspacePage.evaluate(
    ({ projectPath, sessionId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabUpdatePaneUrl !== 'function') {
        throw new Error('Missing tabUpdatePaneUrl API')
      }
      return api.tabUpdatePaneUrl(projectPath, sessionId, 'https://example.test/path')
    },
    { projectPath: tmpDir, sessionId: initialSessionId! },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
                (candidate) => candidate.id === tabId,
              )
              const pane = tab?.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
              return {
                activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
                url: pane?.url ?? null,
              }
            })
          },
          { projectPath: tmpDir, tabId: shellTab!.id },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      activeTabId: shellTab!.id,
      url: 'https://example.test/path',
    })

  await workspacePage.evaluate(
    ({ projectPath, tabId, paneId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabSplitPane !== 'function') {
        throw new Error('Missing tabSplitPane API')
      }
      return api.tabSplitPane(projectPath, tabId, paneId, 'horizontal')
    },
    { projectPath: tmpDir, tabId: shellTab!.id, paneId: initialPaneId! },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate((projectPath) => {
          const api = (window as unknown as { api: Required<AppStateApi> }).api
          return api.getAppState().then((snapshot) => {
            const tabs = snapshot.tabs?.tabsByWorktree[projectPath] ?? []
            const activeTabId = snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null
            const tab = tabs.find((candidate) => candidate.id === activeTabId)
            return {
              splitType: tab?.rootSplit.type ?? null,
              splitId: tab?.rootSplit.type === 'split' ? (tab.rootSplit.id ?? null) : null,
              ratio: tab?.rootSplit.type === 'split' ? (tab.rootSplit.ratio ?? null) : null,
              focusedPaneId: tab?.focusedPaneId ?? null,
              hasTwoChildren:
                tab?.rootSplit.type === 'split' &&
                Boolean(tab.rootSplit.first) &&
                Boolean(tab.rootSplit.second),
            }
          })
        }, tmpDir),
      { timeout: 10_000 },
    )
    .toEqual({
      splitType: 'split',
      splitId: expect.any(String),
      ratio: 0.5,
      focusedPaneId: expect.not.stringMatching(initialPaneId!),
      hasTwoChildren: true,
    })

  const splitPaneId = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    return api.getAppState().then((snapshot) => {
      const activeTabId = snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null
      const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
        (candidate) => candidate.id === activeTabId,
      )
      return tab?.focusedPaneId ?? null
    })
  }, tmpDir)
  expect(splitPaneId).toBeTruthy()
  expect(splitPaneId).not.toBe(initialPaneId)

  const splitId = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    return api.getAppState().then((snapshot) => {
      const activeTabId = snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null
      const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
        (candidate) => candidate.id === activeTabId,
      )
      return tab?.rootSplit.type === 'split' ? (tab.rootSplit.id ?? null) : null
    })
  }, tmpDir)
  expect(splitId).toBeTruthy()

  await workspacePage.evaluate(
    ({ projectPath, tabId, paneId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabFocusPane !== 'function') {
        throw new Error('Missing tabFocusPane API')
      }
      return api.tabFocusPane(projectPath, tabId, paneId)
    },
    { projectPath: tmpDir, tabId: shellTab!.id, paneId: initialPaneId! },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate((projectPath) => {
          const api = (window as unknown as { api: Required<AppStateApi> }).api
          return api.getAppState().then((snapshot) => {
            const activeTabId = snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null
            const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
              (candidate) => candidate.id === activeTabId,
            )
            return tab?.focusedPaneId ?? null
          })
        }, tmpDir),
      { timeout: 10_000 },
    )
    .toBe(initialPaneId)

  await workspacePage.evaluate(
    ({ projectPath, tabId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabNavigatePaneFocus !== 'function') {
        throw new Error('Missing tabNavigatePaneFocus API')
      }
      return api.tabNavigatePaneFocus(projectPath, tabId, 'down')
    },
    { projectPath: tmpDir, tabId: shellTab!.id },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate((projectPath) => {
          const api = (window as unknown as { api: Required<AppStateApi> }).api
          return api.getAppState().then((snapshot) => {
            const activeTabId = snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null
            const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
              (candidate) => candidate.id === activeTabId,
            )
            return tab?.focusedPaneId ?? null
          })
        }, tmpDir),
      { timeout: 10_000 },
    )
    .toBe(splitPaneId)

  await workspacePage.evaluate(
    ({ projectPath, tabId, targetSplitId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabUpdateSplitRatio !== 'function') {
        throw new Error('Missing tabUpdateSplitRatio API')
      }
      return api.tabUpdateSplitRatio(projectPath, tabId, targetSplitId, 0.37)
    },
    { projectPath: tmpDir, tabId: shellTab!.id, targetSplitId: splitId! },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate((projectPath) => {
          const api = (window as unknown as { api: Required<AppStateApi> }).api
          return api.getAppState().then((snapshot) => {
            const activeTabId = snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null
            const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
              (candidate) => candidate.id === activeTabId,
            )
            return tab?.rootSplit.type === 'split' ? (tab.rootSplit.ratio ?? null) : null
          })
        }, tmpDir),
      { timeout: 10_000 },
    )
    .toBe(0.37)

  await workspacePage.evaluate(
    ({ projectPath, tabId, paneId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabClosePane !== 'function') {
        throw new Error('Missing tabClosePane API')
      }
      return api.tabClosePane(projectPath, tabId, paneId)
    },
    { projectPath: tmpDir, tabId: shellTab!.id, paneId: splitPaneId! },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate((projectPath) => {
          const api = (window as unknown as { api: Required<AppStateApi> }).api
          return api.getAppState().then((snapshot) => {
            const activeTabId = snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null
            const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
              (candidate) => candidate.id === activeTabId,
            )
            return {
              splitType: tab?.rootSplit.type ?? null,
              paneId: tab?.rootSplit.type === 'leaf' ? (tab.rootSplit.pane?.id ?? null) : null,
              focusedPaneId: tab?.focusedPaneId ?? null,
            }
          })
        }, tmpDir),
      { timeout: 10_000 },
    )
    .toEqual({
      splitType: 'leaf',
      paneId: initialPaneId,
      focusedPaneId: initialPaneId,
    })

  const secondTab = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.tabOpenTool !== 'function') {
      throw new Error('Missing tabOpenTool API')
    }
    return api.tabOpenTool('shell', projectPath).then((result) => result.openedTab ?? null)
  }, tmpDir)
  expect(secondTab?.id).toBeTruthy()

  await expect
    .poll(
      () =>
        workspacePage.evaluate((projectPath) => {
          const api = (window as unknown as { api: Required<AppStateApi> }).api
          return api.getAppState().then((snapshot) => ({
            tabIds: (snapshot.tabs?.tabsByWorktree[projectPath] ?? []).map((tab) => tab.id),
            activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
          }))
        }, tmpDir),
      { timeout: 10_000 },
    )
    .toEqual({
      tabIds: expect.arrayContaining([secondTab!.id]),
      activeTabId: secondTab!.id,
    })

  const reopenSourceTab = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    return api.tabOpenTool('shell', projectPath).then((result) => result.openedTab ?? null)
  }, tmpDir)
  expect(reopenSourceTab?.id).toBeTruthy()

  await workspacePage.evaluate(
    ({ projectPath, tabId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabCloseTab !== 'function') {
        throw new Error('Missing tabCloseTab API')
      }
      return api.tabCloseTab(projectPath, tabId)
    },
    { projectPath: tmpDir, tabId: reopenSourceTab!.id },
  )

  const reopenedTab = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.tabReopenClosedTab !== 'function') {
      throw new Error('Missing tabReopenClosedTab API')
    }
    return api.tabReopenClosedTab(projectPath).then((result) => result.openedTab ?? null)
  }, tmpDir)
  expect(reopenedTab?.toolId).toBe('shell')
  expect(reopenedTab?.id).toBeTruthy()
  expect(reopenedTab?.id).not.toBe(reopenSourceTab!.id)

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tabs = snapshot.tabs?.tabsByWorktree[projectPath] ?? []
              return {
                present: tabs.some((tab) => tab.id === tabId && tab.toolId === 'shell'),
                activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
              }
            })
          },
          { projectPath: tmpDir, tabId: reopenedTab!.id },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      present: true,
      activeTabId: reopenedTab!.id,
    })

  await workspacePage.evaluate(
    ({ projectPath, tabId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      return api.tabCloseTab(projectPath, tabId)
    },
    { projectPath: tmpDir, tabId: reopenedTab!.id },
  )

  await workspacePage.evaluate(
    ({ projectPath, tabId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabSetActiveTab !== 'function') {
        throw new Error('Missing tabSetActiveTab API')
      }
      return api.tabSetActiveTab(projectPath, tabId)
    },
    { projectPath: tmpDir, tabId: shellTab!.id },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate((projectPath) => {
          const api = (window as unknown as { api: Required<AppStateApi> }).api
          return api
            .getAppState()
            .then((snapshot) => snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null)
        }, tmpDir),
      { timeout: 10_000 },
    )
    .toBe(shellTab!.id)

  const moveExpectation = await workspacePage.evaluate(
    ({ projectPath, tabId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      return api.getAppState().then((snapshot) => {
        const tabIds = (snapshot.tabs?.tabsByWorktree[projectPath] ?? []).map((tab) => tab.id)
        const fromIndex = tabIds.indexOf(tabId)
        if (fromIndex < 0) throw new Error(`Opened tab not found: ${tabId}`)
        const toIndex = fromIndex === 0 ? tabIds.length - 1 : 0
        const expected = [...tabIds]
        const [moved] = expected.splice(fromIndex, 1)
        expected.splice(toIndex, 0, moved)
        return { fromIndex, toIndex, expected }
      })
    },
    { projectPath: tmpDir, tabId: secondTab!.id },
  )

  await workspacePage.evaluate(
    ({ projectPath, fromIndex, toIndex }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabMoveTab !== 'function') {
        throw new Error('Missing tabMoveTab API')
      }
      return api.tabMoveTab(projectPath, fromIndex, toIndex)
    },
    {
      projectPath: tmpDir,
      fromIndex: moveExpectation.fromIndex,
      toIndex: moveExpectation.toIndex,
    },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate((projectPath) => {
          const api = (window as unknown as { api: Required<AppStateApi> }).api
          return api.getAppState().then((snapshot) => ({
            tabIds: (snapshot.tabs?.tabsByWorktree[projectPath] ?? []).map((tab) => tab.id),
            activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
          }))
        }, tmpDir),
      { timeout: 10_000 },
    )
    .toEqual({
      tabIds: moveExpectation.expected,
      activeTabId: shellTab!.id,
    })

  const secondPaneId = await workspacePage.evaluate(
    ({ projectPath, tabId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      return api.getAppState().then((snapshot) => {
        const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
          (candidate) => candidate.id === tabId,
        )
        return tab?.rootSplit.type === 'leaf' ? (tab.rootSplit.pane?.id ?? null) : null
      })
    },
    { projectPath: tmpDir, tabId: secondTab!.id },
  )
  expect(secondPaneId).toBeTruthy()

  await workspacePage.evaluate(
    ({ projectPath, sourceTabId, targetTabId, targetPaneId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabMoveTabToSplit !== 'function') {
        throw new Error('Missing tabMoveTabToSplit API')
      }
      return api.tabMoveTabToSplit(
        projectPath,
        sourceTabId,
        targetTabId,
        targetPaneId,
        'vertical',
        'second',
      )
    },
    {
      projectPath: tmpDir,
      sourceTabId: secondTab!.id,
      targetTabId: shellTab!.id,
      targetPaneId: initialPaneId!,
    },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, targetTabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tabs = snapshot.tabs?.tabsByWorktree[projectPath] ?? []
              const targetTab = tabs.find((candidate) => candidate.id === targetTabId)
              return {
                tabIds: tabs.map((tab) => tab.id),
                activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
                targetSplitType: targetTab?.rootSplit.type ?? null,
                targetDirection:
                  targetTab?.rootSplit.type === 'split'
                    ? (targetTab.rootSplit.direction ?? null)
                    : null,
                focusedPaneId: targetTab?.focusedPaneId ?? null,
              }
            })
          },
          { projectPath: tmpDir, targetTabId: shellTab!.id },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      tabIds: moveExpectation.expected.filter((id) => id !== secondTab!.id),
      activeTabId: shellTab!.id,
      targetSplitType: 'split',
      targetDirection: 'vertical',
      focusedPaneId: secondPaneId,
    })

  await workspacePage.evaluate(
    ({ projectPath, tabId, sourcePaneId, targetPaneId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabMovePaneToTarget !== 'function') {
        throw new Error('Missing tabMovePaneToTarget API')
      }
      return api.tabMovePaneToTarget(
        projectPath,
        tabId,
        sourcePaneId,
        tabId,
        targetPaneId,
        'horizontal',
        'first',
      )
    },
    {
      projectPath: tmpDir,
      tabId: shellTab!.id,
      sourcePaneId: secondPaneId!,
      targetPaneId: initialPaneId!,
    },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
                (candidate) => candidate.id === tabId,
              )
              const root = tab?.rootSplit as
                | {
                    type: 'split'
                    direction?: 'horizontal' | 'vertical'
                    first?: { type?: 'leaf'; pane?: { id: string } }
                  }
                | undefined
              return {
                splitType: tab?.rootSplit.type ?? null,
                direction: root?.type === 'split' ? (root.direction ?? null) : null,
                firstPaneId:
                  root?.type === 'split' && root.first?.type === 'leaf'
                    ? (root.first.pane?.id ?? null)
                    : null,
                focusedPaneId: tab?.focusedPaneId ?? null,
              }
            })
          },
          { projectPath: tmpDir, tabId: shellTab!.id },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      splitType: 'split',
      direction: 'horizontal',
      firstPaneId: secondPaneId,
      focusedPaneId: secondPaneId,
    })

  const detachedTab = await workspacePage.evaluate(
    ({ projectPath, tabId, paneId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabDetachPaneToTab !== 'function') {
        throw new Error('Missing tabDetachPaneToTab API')
      }
      return api
        .tabDetachPaneToTab(projectPath, tabId, paneId)
        .then((result) => result.openedTab ?? null)
    },
    { projectPath: tmpDir, tabId: shellTab!.id, paneId: secondPaneId! },
  )
  expect(detachedTab?.id).toBeTruthy()

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, sourceTabId, detachedTabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tabs = snapshot.tabs?.tabsByWorktree[projectPath] ?? []
              const sourceTab = tabs.find((candidate) => candidate.id === sourceTabId)
              const newTab = tabs.find((candidate) => candidate.id === detachedTabId)
              return {
                activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
                sourceSplitType: sourceTab?.rootSplit.type ?? null,
                sourcePaneId:
                  sourceTab?.rootSplit.type === 'leaf'
                    ? (sourceTab.rootSplit.pane?.id ?? null)
                    : null,
                detachedSplitType: newTab?.rootSplit.type ?? null,
                detachedPaneId:
                  newTab?.rootSplit.type === 'leaf' ? (newTab.rootSplit.pane?.id ?? null) : null,
              }
            })
          },
          { projectPath: tmpDir, sourceTabId: shellTab!.id, detachedTabId: detachedTab!.id },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      activeTabId: detachedTab!.id,
      sourceSplitType: 'leaf',
      sourcePaneId: initialPaneId,
      detachedSplitType: 'leaf',
      detachedPaneId: secondPaneId,
    })

  const diffTab = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.tabOpenDiff !== 'function') {
      throw new Error('Missing tabOpenDiff API')
    }
    return api.tabOpenDiff(projectPath).then((result) => result.openedTab ?? null)
  }, tmpDir)
  expect(diffTab?.id).toBeTruthy()

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, diffTabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tabs = snapshot.tabs?.tabsByWorktree[projectPath] ?? []
              const diffTabs = tabs.filter((tab) => tab.toolId === 'diff')
              const activeTabId = snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null
              const activeTab = tabs.find((tab) => tab.id === activeTabId)
              return {
                diffTabIds: diffTabs.map((tab) => tab.id),
                activeTabId,
                activeToolId: activeTab?.toolId ?? null,
                activePaneToolId:
                  activeTab?.rootSplit.type === 'leaf'
                    ? (activeTab.rootSplit.pane?.toolId ?? null)
                    : null,
                hasDiffTab: diffTabs.some((tab) => tab.id === diffTabId),
              }
            })
          },
          { projectPath: tmpDir, diffTabId: diffTab!.id },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      diffTabIds: [diffTab!.id],
      activeTabId: diffTab!.id,
      activeToolId: 'diff',
      activePaneToolId: 'diff',
      hasDiffTab: true,
    })

  await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    return api.tabOpenDiff(projectPath)
  }, tmpDir)

  await expect
    .poll(
      () =>
        workspacePage.evaluate((projectPath) => {
          const api = (window as unknown as { api: Required<AppStateApi> }).api
          return api.getAppState().then((snapshot) => {
            const tabs = snapshot.tabs?.tabsByWorktree[projectPath] ?? []
            return {
              diffTabIds: tabs.filter((tab) => tab.toolId === 'diff').map((tab) => tab.id),
              activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
            }
          })
        }, tmpDir),
      { timeout: 10_000 },
    )
    .toEqual({
      diffTabIds: [diffTab!.id],
      activeTabId: diffTab!.id,
    })

  const runConfigTab = await workspacePage.evaluate(async (projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.tabOpenSessionTab !== 'function') {
      throw new Error('Missing tabOpenSessionTab API')
    }
    if (typeof api.runConfigSave !== 'function') {
      throw new Error('Missing runConfigSave API')
    }
    if (typeof api.runConfigExecuteCommand !== 'function') {
      throw new Error('Missing runConfigExecuteCommand API')
    }
    await api.runConfigSave(projectPath, {
      configurations: [
        {
          name: 'E2E Command',
          command: 'node',
          args: '-e "setInterval(() => {}, 1000)"',
        },
      ],
    })
    const session = await api.runConfigExecuteCommand(projectPath, 'E2E Command', projectPath)
    const result = await api.tabOpenSessionTab(projectPath, 'E2E Command', session.sessionId)
    return { tab: result.openedTab ?? null, sessionId: session.sessionId, wsUrl: session.wsUrl }
  }, tmpDir)
  expect(runConfigTab.tab?.id).toBeTruthy()
  expect(runConfigTab.sessionId).toBeTruthy()
  expect(runConfigTab.wsUrl).toBe('')

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tabs = snapshot.tabs?.tabsByWorktree[projectPath] ?? []
              const tab = tabs.find((candidate) => candidate.id === tabId)
              const pane = tab?.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
              return {
                activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
                tabToolId: tab?.toolId ?? null,
                tabToolName: tab?.toolName ?? null,
                tabName: tab?.name ?? null,
                paneSessionId: pane?.sessionId ?? null,
                paneWsUrl: pane?.wsUrl ?? null,
                paneToolId: pane?.toolId ?? null,
                paneToolName: pane?.toolName ?? null,
              }
            })
          },
          { projectPath: tmpDir, tabId: runConfigTab.tab!.id },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      activeTabId: runConfigTab.tab!.id,
      tabToolId: 'shell',
      tabToolName: 'E2E Command',
      tabName: 'E2E Command',
      paneSessionId: runConfigTab.sessionId,
      paneWsUrl: runConfigTab.wsUrl,
      paneToolId: 'shell',
      paneToolName: 'Shell',
    })

  const tmuxTab = await openShellTabWithPane(workspacePage, tmpDir)

  await workspacePage.evaluate(
    ({ projectPath, sessionId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabHandlePtyExit !== 'function') {
        throw new Error('Missing tabHandlePtyExit API')
      }
      return api.tabHandlePtyExit(projectPath, sessionId, 0, 'canopy-e2e')
    },
    { projectPath: tmpDir, sessionId: tmuxTab.sessionId },
  )
  expect(tmuxTab.id).toBeTruthy()

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tabs = snapshot.tabs?.tabsByWorktree[projectPath] ?? []
              const tab = tabs.find((candidate) => candidate.id === tabId)
              const pane = tab?.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
              return {
                activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
                tabToolId: tab?.toolId ?? null,
                paneSessionId: pane?.sessionId ?? null,
                paneWsUrl: pane?.wsUrl ?? null,
                paneTmuxSessionName: pane?.tmuxSessionName ?? null,
              }
            })
          },
          { projectPath: tmpDir, tabId: tmuxTab.id },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      activeTabId: tmuxTab.id,
      tabToolId: 'shell',
      paneSessionId: tmuxTab.sessionId,
      paneWsUrl: tmuxTab.wsUrl,
      paneTmuxSessionName: 'canopy-e2e',
    })

  await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.tabUpdateTmuxSessionName !== 'function') {
      throw new Error('Missing tabUpdateTmuxSessionName API')
    }
    return api.tabUpdateTmuxSessionName(projectPath, 'canopy-e2e', 'canopy-e2e-renamed')
  }, tmpDir)

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
                (candidate) => candidate.id === tabId,
              )
              const pane = tab?.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
              return {
                activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
                paneTmuxSessionName: pane?.tmuxSessionName ?? null,
              }
            })
          },
          { projectPath: tmpDir, tabId: tmuxTab.id },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      activeTabId: tmuxTab.id,
      paneTmuxSessionName: 'canopy-e2e-renamed',
    })

  await workspacePage.evaluate(
    ({ projectPath, sessionId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabHandlePtyExit !== 'function') {
        throw new Error('Missing tabHandlePtyExit API')
      }
      return api.tabHandlePtyExit(projectPath, sessionId, 7, 'canopy-e2e-renamed')
    },
    { projectPath: tmpDir, sessionId: tmuxTab.sessionId },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
                (candidate) => candidate.id === tabId,
              )
              const pane = tab?.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
              return {
                activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
                isRunning: pane?.isRunning ?? null,
                exitCode: pane?.exitCode ?? null,
                detached: pane?.detached ?? null,
                paneTmuxSessionName: pane?.tmuxSessionName ?? null,
              }
            })
          },
          { projectPath: tmpDir, tabId: tmuxTab.id },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      activeTabId: tmuxTab.id,
      isRunning: false,
      exitCode: 7,
      detached: false,
      paneTmuxSessionName: 'canopy-e2e-renamed',
    })

  const killTmuxTab = await openShellTabWithPane(workspacePage, tmpDir)

  await workspacePage.evaluate(
    ({ projectPath, sessionId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabHandlePtyExit !== 'function') {
        throw new Error('Missing tabHandlePtyExit API')
      }
      return api.tabHandlePtyExit(projectPath, sessionId, 0, 'canopy-e2e-kill')
    },
    { projectPath: tmpDir, sessionId: killTmuxTab.sessionId },
  )
  expect(killTmuxTab.id).toBeTruthy()

  await workspacePage.evaluate(
    ({ projectPath, tabId, paneId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabKillTmuxPane !== 'function') {
        throw new Error('Missing tabKillTmuxPane API')
      }
      return api.tabKillTmuxPane(projectPath, tabId, paneId)
    },
    { projectPath: tmpDir, tabId: killTmuxTab.id, paneId: killTmuxTab.paneId },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
                (candidate) => candidate.id === tabId,
              )
              const pane = tab?.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
              return {
                activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
                isRunning: pane?.isRunning ?? null,
                detached: pane?.detached ?? null,
                paneTmuxSessionName: pane?.tmuxSessionName ?? null,
              }
            })
          },
          { projectPath: tmpDir, tabId: killTmuxTab.id },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      activeTabId: killTmuxTab.id,
      isRunning: false,
      detached: false,
      paneTmuxSessionName: null,
    })

  const reattachTmuxSessionName = 'canopy-e2e-reattach-missing'
  const reattachTmuxTab = await openShellTabWithPane(workspacePage, tmpDir)
  expect(reattachTmuxTab.id).toBeTruthy()

  await workspacePage.evaluate(
    ({ projectPath, sessionId, tmuxSessionName }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabHandlePtyExit !== 'function') {
        throw new Error('Missing tabHandlePtyExit API')
      }
      return api.tabHandlePtyExit(projectPath, sessionId, 0, tmuxSessionName)
    },
    {
      projectPath: tmpDir,
      sessionId: reattachTmuxTab.sessionId,
      tmuxSessionName: reattachTmuxSessionName,
    },
  )

  await workspacePage.evaluate(
    ({ projectPath, tabId, paneId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabReattachTmuxPane !== 'function') {
        throw new Error('Missing tabReattachTmuxPane API')
      }
      return api.tabReattachTmuxPane(projectPath, tabId, paneId)
    },
    { projectPath: tmpDir, tabId: reattachTmuxTab.id, paneId: reattachTmuxTab.paneId },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId, previousSessionId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
                (candidate) => candidate.id === tabId,
              )
              const pane = tab?.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
              return {
                activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
                isRunning: pane?.isRunning ?? null,
                exitCode: pane?.exitCode ?? null,
                detached: pane?.detached ?? null,
                replacedSessionId: pane?.sessionId !== previousSessionId,
                wsUrl: pane?.wsUrl ?? null,
                retainedMissingTmux: pane?.tmuxSessionName === 'canopy-e2e-reattach-missing',
              }
            })
          },
          {
            projectPath: tmpDir,
            tabId: reattachTmuxTab.id,
            previousSessionId: reattachTmuxTab.sessionId,
          },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      activeTabId: reattachTmuxTab.id,
      isRunning: true,
      exitCode: null,
      detached: false,
      replacedSessionId: true,
      wsUrl: '',
      retainedMissingTmux: false,
    })

  const inspectorTab = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.tabOpenTool !== 'function') {
      throw new Error('Missing tabOpenTool API')
    }
    return api.tabOpenTool('codex', projectPath).then((result) => result.openedTab ?? null)
  }, tmpDir)
  expect(inspectorTab?.id).toBeTruthy()
  const inspectorTabId = inspectorTab!.id

  await workspacePage.evaluate(
    ({ projectPath, tabId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabToggleFocusedInspector !== 'function') {
        throw new Error('Missing tabToggleFocusedInspector API')
      }
      return api.tabToggleFocusedInspector(projectPath, tabId)
    },
    { projectPath: tmpDir, tabId: inspectorTabId },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
                (candidate) => candidate.id === tabId,
              )
              const pane = tab?.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
              return {
                activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
                inspectorOpen: pane?.inspectorOpen ?? null,
              }
            })
          },
          { projectPath: tmpDir, tabId: inspectorTabId },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      activeTabId: inspectorTabId,
      inspectorOpen: true,
    })

  await workspacePage.evaluate(
    ({ projectPath, tabId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      return api.tabToggleFocusedInspector(projectPath, tabId)
    },
    { projectPath: tmpDir, tabId: inspectorTabId },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
                (candidate) => candidate.id === tabId,
              )
              const pane = tab?.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
              return pane?.inspectorOpen ?? null
            })
          },
          { projectPath: tmpDir, tabId: inspectorTabId },
        ),
      { timeout: 10_000 },
    )
    .toBe(false)

  const focusSessionResult = await workspacePage.evaluate((sessionId) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.tabFocusSession !== 'function') {
      throw new Error('Missing tabFocusSession API')
    }
    return api.tabFocusSession(sessionId)
  }, runConfigTab.sessionId)

  expect(focusSessionResult?.activeTabId).toBe(runConfigTab.tab!.id)

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tabs = snapshot.tabs?.tabsByWorktree[projectPath] ?? []
              const tab = tabs.find((candidate) => candidate.id === tabId)
              const pane = tab?.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
              return {
                activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
                focusedPaneId: tab?.focusedPaneId ?? null,
                paneId: pane?.id ?? null,
                paneSessionId: pane?.sessionId ?? null,
              }
            })
          },
          { projectPath: tmpDir, tabId: runConfigTab.tab!.id },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      activeTabId: runConfigTab.tab!.id,
      focusedPaneId: expect.any(String),
      paneId: expect.any(String),
      paneSessionId: runConfigTab.sessionId,
    })

  const closeAllPreflight = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.tabPrepareCloseAllForWorktree !== 'function') {
      throw new Error('Missing tabPrepareCloseAllForWorktree API')
    }
    return api.tabPrepareCloseAllForWorktree(projectPath)
  }, tmpDir)
  expect(closeAllPreflight).toEqual({
    ok: false,
    reason: 'active-processes',
    warnings: [{ tabName: 'E2E Command', description: '1 running process' }],
  })

  const confirmedCloseAllPreflight = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    return api.tabPrepareCloseAllForWorktree(projectPath, { confirmedActiveProcesses: true })
  }, tmpDir)
  expect(confirmedCloseAllPreflight).toEqual({ ok: true })

  const closeAllResult = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.tabCloseAllForWorktree !== 'function') {
      throw new Error('Missing tabCloseAllForWorktree API')
    }
    return api.tabCloseAllForWorktree(projectPath).then((result) => ({
      tabCount: result.tabs.length,
      activeTabId: result.activeTabId,
    }))
  }, tmpDir)

  expect(closeAllResult).toEqual({
    tabCount: 0,
    activeTabId: null,
  })

  const killAllResult = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.tabKillAll !== 'function') {
      throw new Error('Missing tabKillAll API')
    }
    if (typeof api.tabOpenTool !== 'function') {
      throw new Error('Missing tabOpenTool API')
    }
    return api.tabOpenTool('shell', projectPath).then(() => api.tabKillAll())
  }, tmpDir)

  expect(killAllResult).toEqual({
    tabsByWorktree: {},
    activeTabIdByWorktree: {},
  })

  await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.tabOpenTool !== 'function') {
      throw new Error('Missing tabOpenTool API')
    }
    return api.tabOpenTool('shell', projectPath)
  }, tmpDir)

  await expect
    .poll(
      () =>
        workspacePage.evaluate((projectPath) => {
          const api = (window as unknown as { api: Required<AppStateApi> }).api
          return api.getAppState().then((snapshot) => {
            const tabs = snapshot.tabs?.tabsByWorktree[projectPath] ?? []
            const activeTabId = snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null
            const activeTab = tabs.find((tab) => tab.id === activeTabId)
            return {
              hasTabs: tabs.length > 0,
              activeToolId: activeTab?.toolId ?? null,
            }
          })
        }, tmpDir),
      { timeout: 10_000 },
    )
    .toEqual({
      hasTabs: true,
      activeToolId: 'shell',
    })

  const restoreLayoutResult = await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    if (typeof api.tabRestoreLayout !== 'function') {
      throw new Error('Missing tabRestoreLayout API')
    }
    return api.tabRestoreLayout(
      projectPath,
      JSON.stringify({
        activeTabIndex: 0,
        tabs: [
          {
            toolId: 'browser',
            toolName: 'Browser',
            rootSplit: {
              type: 'leaf',
              toolId: 'browser',
              toolName: 'Browser',
              browserUrl: 'https://example.test/restored',
            },
          },
          {
            toolId: 'shell',
            toolName: 'Shell',
            rootSplit: {
              type: 'leaf',
              toolId: 'shell',
              toolName: 'Shell',
            },
          },
          {
            toolId: 'shell',
            toolName: 'Shell',
            rootSplit: {
              type: 'leaf',
              toolId: 'shell',
              toolName: 'Shell',
            },
          },
        ],
      }),
    )
  }, tmpDir)

  expect(restoreLayoutResult.restored).toBe(true)
  expect(restoreLayoutResult.tabs).toHaveLength(3)

  await expect
    .poll(
      () =>
        workspacePage.evaluate((projectPath) => {
          const api = (window as unknown as { api: Required<AppStateApi> }).api
          return api.getAppState().then((snapshot) => {
            const tabs = snapshot.tabs?.tabsByWorktree[projectPath] ?? []
            const activeTabId = snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null
            const activeTab = tabs.find((tab) => tab.id === activeTabId)
            const suspendedTab = tabs.find((tab) => tab.id !== activeTabId)
            const activePane =
              activeTab?.rootSplit.type === 'leaf' ? activeTab.rootSplit.pane : null
            const suspendedPane =
              suspendedTab?.rootSplit.type === 'leaf' ? suspendedTab.rootSplit.pane : null

            return {
              tabCount: tabs.length,
              activeToolId: activeTab?.toolId ?? null,
              activePaneType: activePane?.paneType ?? null,
              activeUrl: activePane?.url ?? null,
              suspendedCount: tabs.filter((tab) => Boolean(tab.suspended)).length,
              firstSuspendedToolId: suspendedTab?.toolId ?? null,
              firstSuspended: Boolean(suspendedTab?.suspended),
              firstSuspendedRunning: suspendedPane?.isRunning ?? null,
            }
          })
        }, tmpDir),
      { timeout: 10_000 },
    )
    .toEqual({
      tabCount: 3,
      activeToolId: 'browser',
      activePaneType: 'browser',
      activeUrl: 'https://example.test/restored',
      suspendedCount: 2,
      firstSuspendedToolId: 'shell',
      firstSuspended: true,
      firstSuspendedRunning: false,
    })

  const suspendedRestoreTabIds = restoreLayoutResult.tabs
    .filter((tab) => Boolean(tab.suspended))
    .map((tab) => tab.id)
  const suspendedCloseTabId = suspendedRestoreTabIds[0] ?? null
  const suspendedRestoreTabId = suspendedRestoreTabIds[1] ?? null
  expect(suspendedCloseTabId).toBeTruthy()
  expect(suspendedRestoreTabId).toBeTruthy()

  await workspacePage.evaluate(
    ({ projectPath, tabId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabCloseTab !== 'function') {
        throw new Error('Missing tabCloseTab API')
      }
      return api.tabCloseTab(projectPath, tabId)
    },
    { projectPath: tmpDir, tabId: suspendedCloseTabId! },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, closedTabId, remainingSuspendedTabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tabs = snapshot.tabs?.tabsByWorktree[projectPath] ?? []
              return {
                tabCount: tabs.length,
                activeToolId:
                  tabs.find((tab) => tab.id === snapshot.tabs?.activeTabIdByWorktree[projectPath])
                    ?.toolId ?? null,
                closedTabPresent: tabs.some((tab) => tab.id === closedTabId),
                remainingSuspended: Boolean(
                  tabs.find((tab) => tab.id === remainingSuspendedTabId)?.suspended,
                ),
              }
            })
          },
          {
            projectPath: tmpDir,
            closedTabId: suspendedCloseTabId!,
            remainingSuspendedTabId: suspendedRestoreTabId!,
          },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      tabCount: 2,
      activeToolId: 'browser',
      closedTabPresent: false,
      remainingSuspended: true,
    })

  await workspacePage.evaluate(
    ({ projectPath, tabId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabResumeSuspendedTab !== 'function') {
        throw new Error('Missing tabResumeSuspendedTab API')
      }
      return api.tabResumeSuspendedTab(projectPath, tabId)
    },
    { projectPath: tmpDir, tabId: suspendedRestoreTabId! },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
                (candidate) => candidate.id === tabId,
              )
              const pane = tab?.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
              return {
                suspended: Boolean(tab?.suspended),
                isRunning: pane?.isRunning ?? null,
                sessionId: pane?.sessionId ?? null,
                wsUrl: pane?.wsUrl ?? null,
              }
            })
          },
          { projectPath: tmpDir, tabId: suspendedRestoreTabId! },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      suspended: false,
      isRunning: true,
      sessionId: expect.any(String),
      wsUrl: '',
    })

  const editorFilePath = join(tmpDir, 'README.md')
  const secondEditorFilePath = join(tmpDir, 'DIRTY.txt')
  const editorTab = await workspacePage.evaluate(
    ({ projectPath, filePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabOpenEditorFile !== 'function') {
        throw new Error('Missing tabOpenEditorFile API')
      }
      return api.tabOpenEditorFile(projectPath, filePath).then((result) => ({
        openedTabId: result.openedTab?.id ?? null,
        activeTabId: result.activeTabId,
      }))
    },
    { projectPath: tmpDir, filePath: editorFilePath },
  )

  expect(editorTab.openedTabId).toBeTruthy()
  expect(editorTab.activeTabId).toBe(editorTab.openedTabId)

  const secondOpenResult = await workspacePage.evaluate(
    ({ projectPath, filePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      return api.tabOpenEditorFile(projectPath, filePath).then((result) => ({
        openedTabId: result.openedTab?.id ?? null,
        activeTabId: result.activeTabId,
      }))
    },
    { projectPath: tmpDir, filePath: secondEditorFilePath },
  )

  expect(secondOpenResult.openedTabId).toBeNull()
  expect(secondOpenResult.activeTabId).toBe(editorTab.openedTabId)

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tabs = snapshot.tabs?.tabsByWorktree[projectPath] ?? []
              const editorTabs = tabs.filter((tab) => tab.toolId === 'editor')
              const tab = editorTabs.find((candidate) => candidate.id === tabId)
              const pane = tab?.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
              return {
                editorTabIds: editorTabs.map((candidate) => candidate.id),
                activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
                paneToolId: pane?.toolId ?? null,
                filePath: pane?.filePath ?? null,
                editorActiveFile: pane?.editorActiveFile ?? null,
                editorFiles: pane?.editorFiles?.map((file) => file.filePath) ?? [],
              }
            })
          },
          { projectPath: tmpDir, tabId: editorTab.openedTabId },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      editorTabIds: [editorTab.openedTabId],
      activeTabId: editorTab.openedTabId,
      paneToolId: 'editor',
      filePath: secondEditorFilePath,
      editorActiveFile: secondEditorFilePath,
      editorFiles: [editorFilePath, secondEditorFilePath],
    })

  const editorPaneId = await workspacePage.evaluate(
    ({ projectPath, tabId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      return api.getAppState().then((snapshot) => {
        const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
          (candidate) => candidate.id === tabId,
        )
        return tab?.rootSplit.type === 'leaf' ? (tab.rootSplit.pane?.id ?? null) : null
      })
    },
    { projectPath: tmpDir, tabId: editorTab.openedTabId },
  )
  expect(editorPaneId).toBeTruthy()

  const loadedEditorFile = await workspacePage.evaluate(
    ({ projectPath, paneId, filePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabLoadEditorFile !== 'function') {
        throw new Error('Missing tabLoadEditorFile API')
      }
      return api.tabLoadEditorFile(projectPath, paneId, filePath, { maxBytes: 2_097_152 })
    },
    { projectPath: tmpDir, paneId: editorPaneId, filePath: editorFilePath },
  )
  expect(loadedEditorFile).toMatchObject({
    ok: true,
    binary: false,
    content: '# Test Project\n',
    truncated: false,
    size: 15,
    canWrite: true,
    fileLineEnding: 'LF',
  })
  if (loadedEditorFile.ok) {
    expect(loadedEditorFile.mtimeMs).toBeGreaterThan(0)
  }

  const detachedEditorTab = await workspacePage.evaluate(
    ({ projectPath, paneId, filePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabDetachEditorFile !== 'function') {
        throw new Error('Missing tabDetachEditorFile API')
      }
      return api.tabDetachEditorFile(projectPath, paneId, filePath).then((result) => ({
        openedTabId: result.openedTab?.id ?? null,
        activeTabId: result.activeTabId,
      }))
    },
    { projectPath: tmpDir, paneId: editorPaneId, filePath: secondEditorFilePath },
  )
  expect(detachedEditorTab.openedTabId).toBeTruthy()
  expect(detachedEditorTab.activeTabId).toBe(detachedEditorTab.openedTabId)

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, sourceTabId, detachedTabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tabs = snapshot.tabs?.tabsByWorktree[projectPath] ?? []
              const sourceTab = tabs.find((candidate) => candidate.id === sourceTabId)
              const detachedTab = tabs.find((candidate) => candidate.id === detachedTabId)
              const sourcePane =
                sourceTab?.rootSplit.type === 'leaf' ? sourceTab.rootSplit.pane : null
              const detachedPane =
                detachedTab?.rootSplit.type === 'leaf' ? detachedTab.rootSplit.pane : null
              return {
                activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
                editorTabIds: tabs.filter((tab) => tab.toolId === 'editor').map((tab) => tab.id),
                sourceFiles: sourcePane?.editorFiles?.map((file) => file.filePath) ?? [],
                sourceActiveFile: sourcePane?.editorActiveFile ?? null,
                detachedFiles: detachedPane?.editorFiles?.map((file) => file.filePath) ?? [],
                detachedActiveFile: detachedPane?.editorActiveFile ?? null,
              }
            })
          },
          {
            projectPath: tmpDir,
            sourceTabId: editorTab.openedTabId,
            detachedTabId: detachedEditorTab.openedTabId,
          },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      activeTabId: detachedEditorTab.openedTabId,
      editorTabIds: [editorTab.openedTabId, detachedEditorTab.openedTabId],
      sourceFiles: [editorFilePath],
      sourceActiveFile: editorFilePath,
      detachedFiles: [secondEditorFilePath],
      detachedActiveFile: secondEditorFilePath,
    })

  const detachedEditorPaneId = await workspacePage.evaluate(
    ({ projectPath, tabId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      return api.getAppState().then((snapshot) => {
        const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
          (candidate) => candidate.id === tabId,
        )
        return tab?.rootSplit.type === 'leaf' ? (tab.rootSplit.pane?.id ?? null) : null
      })
    },
    { projectPath: tmpDir, tabId: detachedEditorTab.openedTabId },
  )
  expect(detachedEditorPaneId).toBeTruthy()

  const closeEditorFileResult = await workspacePage.evaluate(
    ({ projectPath, paneId, filePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabCloseEditorFile !== 'function') {
        throw new Error('Missing tabCloseEditorFile API')
      }
      return api.tabCloseEditorFile(projectPath, paneId, filePath)
    },
    { projectPath: tmpDir, paneId: detachedEditorPaneId, filePath: secondEditorFilePath },
  )
  expect(closeEditorFileResult.closedTabId).toBe(detachedEditorTab.openedTabId)
  expect(closeEditorFileResult.activeTabId).toBe(editorTab.openedTabId)

  await expect
    .poll(
      () =>
        workspacePage.evaluate((projectPath) => {
          const api = (window as unknown as { api: Required<AppStateApi> }).api
          return api.getAppState().then((snapshot) => {
            const tabs = snapshot.tabs?.tabsByWorktree[projectPath] ?? []
            return {
              activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
              editorTabIds: tabs.filter((tab) => tab.toolId === 'editor').map((tab) => tab.id),
            }
          })
        }, tmpDir),
      { timeout: 10_000 },
    )
    .toEqual({
      activeTabId: editorTab.openedTabId,
      editorTabIds: [editorTab.openedTabId],
    })

  await workspacePage.evaluate(
    ({ projectPath, filePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      return api.tabOpenEditorFile(projectPath, filePath)
    },
    { projectPath: tmpDir, filePath: secondEditorFilePath },
  )

  await workspacePage.evaluate(
    ({ projectPath, paneId, filePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabMoveEditorFile !== 'function') {
        throw new Error('Missing tabMoveEditorFile API')
      }
      return api.tabMoveEditorFile(projectPath, paneId, filePath, 2)
    },
    { projectPath: tmpDir, paneId: editorPaneId, filePath: editorFilePath },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
                (candidate) => candidate.id === tabId,
              )
              const pane = tab?.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
              return {
                activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
                editorFiles: pane?.editorFiles?.map((file) => file.filePath) ?? [],
              }
            })
          },
          { projectPath: tmpDir, tabId: editorTab.openedTabId },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      activeTabId: editorTab.openedTabId,
      editorFiles: [secondEditorFilePath, editorFilePath],
    })

  await workspacePage.evaluate(
    ({ projectPath, paneId, filePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabSetActiveEditorFile !== 'function') {
        throw new Error('Missing tabSetActiveEditorFile API')
      }
      return api.tabSetActiveEditorFile(projectPath, paneId, filePath)
    },
    { projectPath: tmpDir, paneId: editorPaneId, filePath: secondEditorFilePath },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
                (candidate) => candidate.id === tabId,
              )
              const pane = tab?.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
              return {
                activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
                editorActiveFile: pane?.editorActiveFile ?? null,
                filePath: pane?.filePath ?? null,
              }
            })
          },
          { projectPath: tmpDir, tabId: editorTab.openedTabId },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      activeTabId: editorTab.openedTabId,
      editorActiveFile: secondEditorFilePath,
      filePath: secondEditorFilePath,
    })

  await workspacePage.evaluate(
    ({ projectPath, paneId, filePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabUpdateEditorFileState !== 'function') {
        throw new Error('Missing tabUpdateEditorFileState API')
      }
      return api.tabUpdateEditorFileState(projectPath, paneId, filePath, {
        dirty: true,
        currentContent: 'changed in main',
        externalChangeDetected: true,
      })
    },
    { projectPath: tmpDir, paneId: editorPaneId, filePath: secondEditorFilePath },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId, filePath }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
                (candidate) => candidate.id === tabId,
              )
              const pane = tab?.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
              const editorFiles = pane?.editorFiles as Array<Record<string, unknown>> | undefined
              const file = editorFiles?.find((candidate) => candidate.filePath === filePath)
              return {
                activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
                filePath: file?.filePath ?? null,
                dirty: file?.dirty ?? null,
                currentContent: file?.currentContent ?? null,
                externalChangeDetected: file?.externalChangeDetected ?? null,
              }
            })
          },
          { projectPath: tmpDir, tabId: editorTab.openedTabId, filePath: secondEditorFilePath },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      activeTabId: editorTab.openedTabId,
      filePath: secondEditorFilePath,
      dirty: true,
      currentContent: 'changed in main',
      externalChangeDetected: true,
    })

  const saveEditorFileResult = await workspacePage.evaluate(
    ({ projectPath, paneId, filePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabSaveEditorFile !== 'function') {
        throw new Error('Missing tabSaveEditorFile API')
      }
      return api.tabSaveEditorFile(projectPath, paneId, filePath, {
        content: 'changed in main',
        fileLineEnding: 'LF',
      })
    },
    { projectPath: tmpDir, paneId: editorPaneId, filePath: secondEditorFilePath },
  )
  expect(saveEditorFileResult.ok).toBe(true)
  expect(await readFile(secondEditorFilePath, 'utf8')).toBe('changed in main')

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId, filePath }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
                (candidate) => candidate.id === tabId,
              )
              const pane = tab?.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
              const editorFiles = pane?.editorFiles as Array<Record<string, unknown>> | undefined
              const file = editorFiles?.find((candidate) => candidate.filePath === filePath)
              return {
                activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
                dirty: file?.dirty ?? null,
                currentContent: file?.currentContent ?? null,
                originalContent: file?.originalContent ?? null,
                externalChangeDetected: file?.externalChangeDetected ?? null,
              }
            })
          },
          { projectPath: tmpDir, tabId: editorTab.openedTabId, filePath: secondEditorFilePath },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      activeTabId: editorTab.openedTabId,
      dirty: false,
      currentContent: 'changed in main',
      originalContent: 'changed in main',
      externalChangeDetected: false,
    })

  await workspacePage.evaluate(
    ({ projectPath, paneId, filePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      return api.tabUpdateEditorFileState(projectPath, paneId, filePath, {
        dirty: true,
        currentContent: 'changed by tab preflight',
      })
    },
    { projectPath: tmpDir, paneId: editorPaneId, filePath: secondEditorFilePath },
  )

  await electronApp.evaluate(({ dialog }) => {
    const globals = globalThis as typeof globalThis & {
      __canopyE2eShowMessageBox?: typeof dialog.showMessageBox
    }
    globals.__canopyE2eShowMessageBox ??= dialog.showMessageBox
    dialog.showMessageBox = async () => ({ response: 0, checkboxChecked: false })
  })

  const closeTabPreflight = await workspacePage.evaluate(
    ({ projectPath, tabId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabPrepareCloseTab !== 'function') {
        throw new Error('Missing tabPrepareCloseTab API')
      }
      return api.tabPrepareCloseTab(projectPath, tabId)
    },
    { projectPath: tmpDir, tabId: editorTab.openedTabId },
  )

  await electronApp.evaluate(({ dialog }) => {
    const globals = globalThis as typeof globalThis & {
      __canopyE2eShowMessageBox?: typeof dialog.showMessageBox
    }
    if (globals.__canopyE2eShowMessageBox) {
      dialog.showMessageBox = globals.__canopyE2eShowMessageBox
      delete globals.__canopyE2eShowMessageBox
    }
  })

  expect(closeTabPreflight).toEqual({ ok: true })
  expect(await readFile(secondEditorFilePath, 'utf8')).toBe('changed by tab preflight')

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId, filePath }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
                (candidate) => candidate.id === tabId,
              )
              const pane = tab?.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
              const editorFiles = pane?.editorFiles as Array<Record<string, unknown>> | undefined
              const file = editorFiles?.find((candidate) => candidate.filePath === filePath)
              return {
                dirty: file?.dirty ?? null,
                currentContent: file?.currentContent ?? null,
                originalContent: file?.originalContent ?? null,
              }
            })
          },
          { projectPath: tmpDir, tabId: editorTab.openedTabId, filePath: secondEditorFilePath },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      dirty: false,
      currentContent: 'changed by tab preflight',
      originalContent: 'changed by tab preflight',
    })

  await workspacePage.evaluate(
    ({ projectPath, paneId, filePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      return api.tabUpdateEditorFileState(projectPath, paneId, filePath, {
        dirty: true,
        currentContent: 'changed for move',
      })
    },
    { projectPath: tmpDir, paneId: editorPaneId, filePath: secondEditorFilePath },
  )

  await electronApp.evaluate(({ dialog }) => {
    const globals = globalThis as typeof globalThis & {
      __canopyE2eShowMessageBox?: typeof dialog.showMessageBox
    }
    globals.__canopyE2eShowMessageBox ??= dialog.showMessageBox
    dialog.showMessageBox = async () => ({ response: 0, checkboxChecked: false })
  })

  const closeEditorFilePreflight = await workspacePage.evaluate(
    ({ projectPath, paneId, filePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabPrepareCloseEditorFile !== 'function') {
        throw new Error('Missing tabPrepareCloseEditorFile API')
      }
      return api.tabPrepareCloseEditorFile(projectPath, paneId, filePath)
    },
    { projectPath: tmpDir, paneId: editorPaneId, filePath: secondEditorFilePath },
  )

  await electronApp.evaluate(({ dialog }) => {
    const globals = globalThis as typeof globalThis & {
      __canopyE2eShowMessageBox?: typeof dialog.showMessageBox
    }
    if (globals.__canopyE2eShowMessageBox) {
      dialog.showMessageBox = globals.__canopyE2eShowMessageBox
      delete globals.__canopyE2eShowMessageBox
    }
  })

  expect(closeEditorFilePreflight).toEqual({ ok: true })
  expect(await readFile(secondEditorFilePath, 'utf8')).toBe('changed for move')

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId, filePath }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
                (candidate) => candidate.id === tabId,
              )
              const pane = tab?.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
              const editorFiles = pane?.editorFiles as Array<Record<string, unknown>> | undefined
              const file = editorFiles?.find((candidate) => candidate.filePath === filePath)
              return {
                dirty: file?.dirty ?? null,
                currentContent: file?.currentContent ?? null,
                originalContent: file?.originalContent ?? null,
              }
            })
          },
          { projectPath: tmpDir, tabId: editorTab.openedTabId, filePath: secondEditorFilePath },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      dirty: false,
      currentContent: 'changed for move',
      originalContent: 'changed for move',
    })

  await workspacePage.evaluate(
    ({ projectPath, paneId, filePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      return api.tabUpdateEditorFileState(projectPath, paneId, filePath, {
        dirty: true,
        currentContent: 'changed for move',
      })
    },
    { projectPath: tmpDir, paneId: editorPaneId, filePath: secondEditorFilePath },
  )

  const detachedForMove = await workspacePage.evaluate(
    ({ projectPath, paneId, filePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      return api
        .tabDetachEditorFile(projectPath, paneId, filePath)
        .then((result) => result.openedTab ?? null)
    },
    { projectPath: tmpDir, paneId: editorPaneId, filePath: secondEditorFilePath },
  )
  expect(detachedForMove?.id).toBeTruthy()

  const sourceEditorPaneId = await workspacePage.evaluate(
    ({ projectPath, tabId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      return api.getAppState().then((snapshot) => {
        const tab = snapshot.tabs?.tabsByWorktree[projectPath]?.find(
          (candidate) => candidate.id === tabId,
        )
        return tab?.rootSplit.type === 'leaf' ? (tab.rootSplit.pane?.id ?? null) : null
      })
    },
    { projectPath: tmpDir, tabId: detachedForMove!.id },
  )
  expect(sourceEditorPaneId).toBeTruthy()

  await workspacePage.evaluate(
    ({ projectPath, sourceTabId, targetTabId, targetPaneId }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      return api.tabMoveTabToSplit(
        projectPath,
        sourceTabId,
        targetTabId,
        targetPaneId,
        'horizontal',
        'second',
      )
    },
    {
      projectPath: tmpDir,
      sourceTabId: detachedForMove!.id,
      targetTabId: editorTab.openedTabId!,
      targetPaneId: editorPaneId,
    },
  )

  await workspacePage.evaluate(
    ({ projectPath, sourcePaneId, targetPaneId, filePath }) => {
      const api = (window as unknown as { api: Required<AppStateApi> }).api
      if (typeof api.tabMoveEditorFileBetweenPanes !== 'function') {
        throw new Error('Missing tabMoveEditorFileBetweenPanes API')
      }
      return api.tabMoveEditorFileBetweenPanes(projectPath, sourcePaneId, targetPaneId, filePath, 0)
    },
    {
      projectPath: tmpDir,
      sourcePaneId: sourceEditorPaneId!,
      targetPaneId: editorPaneId,
      filePath: secondEditorFilePath,
    },
  )

  await expect
    .poll(
      () =>
        workspacePage.evaluate(
          ({ projectPath, tabId }) => {
            const api = (window as unknown as { api: Required<AppStateApi> }).api
            return api.getAppState().then((snapshot) => {
              const tabs = snapshot.tabs?.tabsByWorktree[projectPath] ?? []
              const tab = tabs.find((candidate) => candidate.id === tabId)
              const pane = tab?.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
              const editorFiles = pane?.editorFiles as Array<Record<string, unknown>> | undefined
              return {
                activeTabId: snapshot.tabs?.activeTabIdByWorktree[projectPath] ?? null,
                tabIds: tabs
                  .filter((candidate) => candidate.toolId === 'editor')
                  .map((candidate) => candidate.id),
                splitType: tab?.rootSplit.type ?? null,
                editorFiles: editorFiles?.map((file) => file.filePath) ?? [],
                movedDirty: editorFiles?.[0]?.dirty ?? null,
              }
            })
          },
          { projectPath: tmpDir, tabId: editorTab.openedTabId },
        ),
      { timeout: 10_000 },
    )
    .toEqual({
      activeTabId: editorTab.openedTabId,
      tabIds: [editorTab.openedTabId],
      splitType: 'leaf',
      editorFiles: [secondEditorFilePath, editorFilePath],
      movedDirty: true,
    })

  const changedStatePromise = workspacePage.evaluate(
    (projectPath) =>
      new Promise<AppStateSnapshot>((resolve, reject) => {
        const api = (window as unknown as { api: AppStateApi }).api
        if (typeof api.onAppStateChanged !== 'function') {
          reject(new Error('Missing app state subscription API'))
          return
        }

        const cleanupRef: { current?: () => void } = {}
        const timeoutId = window.setTimeout(() => {
          cleanupRef.current?.()
          reject(new Error('Timed out waiting for app state change'))
        }, 10_000)

        cleanupRef.current = api.onAppStateChanged((snapshot) => {
          const projectAttached = snapshot.workspace.projects.some(
            (project) => project.workspace.path === projectPath || project.repoRoot === projectPath,
          )
          if (!projectAttached) return
          window.clearTimeout(timeoutId)
          cleanupRef.current?.()
          resolve(snapshot)
        })
      }),
    tmpDir,
  )

  await workspacePage.evaluate((projectPath) => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    return api.workspaceSelectWorktree(projectPath)
  }, tmpDir)

  const changedState = await changedStatePromise
  expect(hasProject(changedState, tmpDir)).toBe(true)
  expect(changedState.workspace.workspaceState.selectedWorktreePath).toBe(tmpDir)
  expect(changedState.workspace.workspaceState.isDirty).toBe(true)

  const currentState = await workspacePage.evaluate(() => {
    const api = (window as unknown as { api: Required<AppStateApi> }).api
    return api.getAppState()
  })

  expect(hasProject(currentState, tmpDir)).toBe(true)
  expect(currentState.workspace.workspaceState.selectedWorktreePath).toBe(tmpDir)
  expect(currentState.workspace.workspaceState.isDirty).toBe(true)
})
