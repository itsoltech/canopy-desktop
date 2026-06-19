import type { WebContents } from 'electron'
import { execFile } from 'child_process'
import { promisify } from 'util'
import path from 'path'
import type { WorkspaceStore } from '../db/WorkspaceStore'
import type { LayoutStore } from '../db/LayoutStore'
import type { WindowManager } from '../WindowManager'
import type { WorkspaceRow } from '../db/types'
import { GitRepository, type GitInfo } from '../git/GitRepository'
import { GitWatcher, type GitRefreshFlags } from '../git/GitWatcher'
import { gitErrorMessage } from '../git/errors'
import type {
  CommandWarning,
  ProjectSnapshot,
  WorkspaceCommandResult,
  WorkspaceSnapshot,
  WorkspaceStateSnapshot,
} from './types'

const execFileAsync = promisify(execFile)

const defaultGitInfo: GitInfo = {
  isGitRepo: false,
  repoRoot: null,
  branch: null,
  worktrees: [],
  isDirty: false,
  aheadBehind: null,
}

interface WorkspaceCommandServiceDeps {
  workspaceStore: WorkspaceStore
  layoutStore: LayoutStore
  windowManager: WindowManager
  persistWindowConfigs: () => void
  validatePathAccess: (webContentsId: number, targetPath: string) => Promise<string>
  clearWorkspaceFileCache: (path: string) => void
  emitAppStateChanged: (sender: WebContents) => void
}

interface AttachProjectOptions {
  selectIfEmpty?: boolean
  warnings: CommandWarning[]
  restoredLayouts: Array<{ worktreePath: string; layoutJson: string }>
}

type WorktreeStatusSnapshot = Pick<WorkspaceStateSnapshot, 'branch' | 'isDirty' | 'aheadBehind'>

export class WorkspaceCommandService {
  private projectsByWindow = new Map<number, ProjectSnapshot[]>()
  private selectedWorktreeByWindow = new Map<number, string>()
  private statusByWorktreeByWindow = new Map<number, Map<string, WorktreeStatusSnapshot>>()
  private grantedAttachPathsByWindow = new Map<number, Set<string>>()
  private trackedWebContents = new Set<number>()

  constructor(private deps: WorkspaceCommandServiceDeps) {}

  async restoreWindow(
    sender: WebContents,
    payload: { paths: string[]; activeWorktreePath?: string; removedPaths?: string[] },
  ): Promise<WorkspaceCommandResult> {
    this.trackSender(sender)
    const warnings: CommandWarning[] = []
    const restoredLayouts: Array<{ worktreePath: string; layoutJson: string }> = []

    if (payload.removedPaths && payload.removedPaths.length > 0) {
      warnings.push(this.stalePathsWarning(payload.removedPaths))
    }

    let focusedExistingWindow = false
    for (const projectPath of new Set(payload.paths)) {
      focusedExistingWindow =
        (await this.attachProjectSnapshot(sender, projectPath, {
          selectIfEmpty: false,
          warnings,
          restoredLayouts,
        }).catch((err) => {
          const name = path.basename(projectPath) || projectPath
          warnings.push({
            code: 'layout-ignored',
            message: `Failed to restore ${name}: ${err instanceof Error ? err.message : String(err)}`,
            paths: [projectPath],
          })
          return false
        })) || focusedExistingWindow
    }

    const projects = this.getProjects(sender.id)
    let targetPath = payload.activeWorktreePath
    if (!targetPath || !this.getProjectForWorktree(sender.id, targetPath)) {
      targetPath = this.getDefaultWorktreePath(projects[0])
    }

    if (targetPath) {
      this.selectedWorktreeByWindow.set(sender.id, targetPath)
      this.deps.windowManager.setActiveWorktree(sender.id, targetPath)
      await this.refreshSelectedWorktreeStatus(sender.id, targetPath)
      this.deps.persistWindowConfigs()
    }

    const result = this.result(sender.id, warnings, restoredLayouts)
    if (focusedExistingWindow) result.focusedExistingWindow = true
    this.emitAppStateChanged(sender)
    return result
  }

  async attachProject(sender: WebContents, projectPath: string): Promise<WorkspaceCommandResult> {
    this.trackSender(sender)
    const warnings: CommandWarning[] = []
    const restoredLayouts: Array<{ worktreePath: string; layoutJson: string }> = []
    const focusedExistingWindow = await this.attachProjectSnapshot(sender, projectPath, {
      selectIfEmpty: true,
      warnings,
      restoredLayouts,
    })
    const result = this.result(sender.id, warnings, restoredLayouts)
    if (focusedExistingWindow) result.focusedExistingWindow = true
    this.emitAppStateChanged(sender)
    return result
  }

  detachProject(sender: WebContents, projectPath: string): WorkspaceCommandResult {
    this.trackSender(sender)
    const projects = this.getProjects(sender.id)
    const project = projects.find((p) => this.projectKey(p) === projectPath)
    if (!project) return this.result(sender.id, [], [])

    const key = this.projectKey(project)
    this.deps.windowManager.removeWorkspacePath(sender.id, key)
    if (project.repoRoot) this.deps.windowManager.disposeGitWatcher(sender.id, project.repoRoot)
    this.clearProjectStatus(sender.id, project)

    const ws = this.deps.workspaceStore.getByPath(key)
    if (ws) this.deps.layoutStore.deleteAll(ws.id)
    this.deps.clearWorkspaceFileCache(key)

    this.projectsByWindow.set(
      sender.id,
      projects.filter((candidate) => this.projectKey(candidate) !== key),
    )

    const remaining = this.getProjects(sender.id)
    const selectedPath = this.selectedWorktreeByWindow.get(sender.id)
    if (selectedPath && this.projectOwnsPath(project, selectedPath)) {
      const fallbackPath = this.getDefaultWorktreePath(remaining[0])
      if (fallbackPath) {
        this.selectedWorktreeByWindow.set(sender.id, fallbackPath)
        this.deps.windowManager.setActiveWorktree(sender.id, fallbackPath)
      } else {
        this.selectedWorktreeByWindow.delete(sender.id)
        this.deps.windowManager.clearActiveWorktree(sender.id)
      }
    }

    this.deps.persistWindowConfigs()
    const result = this.result(sender.id, [], [])
    this.emitAppStateChanged(sender)
    return result
  }

  async selectWorktree(sender: WebContents, worktreePath: string): Promise<WorkspaceCommandResult> {
    this.trackSender(sender)
    if (!this.getProjectForWorktree(sender.id, worktreePath)) {
      await this.refreshProjectForPath(sender.id, worktreePath)
    }
    if (!this.getProjectForWorktree(sender.id, worktreePath)) {
      throw new Error(`Worktree is not attached to this window: ${worktreePath}`)
    }
    this.selectedWorktreeByWindow.set(sender.id, worktreePath)
    this.deps.windowManager.setActiveWorktree(sender.id, worktreePath)
    await this.refreshSelectedWorktreeStatus(sender.id, worktreePath)
    this.deps.persistWindowConfigs()
    const result = this.result(sender.id, [], [])
    this.emitAppStateChanged(sender)
    return result
  }

  getSnapshot(webContentsId: number): WorkspaceSnapshot {
    return {
      projects: this.getProjects(webContentsId),
      workspaceState: this.workspaceState(webContentsId),
    }
  }

  getWorkspaceIdForWorktree(webContentsId: number, worktreePath: string): string | null {
    return this.getProjectForWorktree(webContentsId, worktreePath)?.workspace.id ?? null
  }

  grantAttachPath(webContentsId: number, targetPath: string): void {
    let paths = this.grantedAttachPathsByWindow.get(webContentsId)
    if (!paths) {
      paths = new Set()
      this.grantedAttachPathsByWindow.set(webContentsId, paths)
    }
    paths.add(targetPath)
  }

  getGrantedAttachPaths(webContentsId: number): string[] {
    return [...(this.grantedAttachPathsByWindow.get(webContentsId) ?? [])]
  }

  async initGitRepo(sender: WebContents, projectPath: string): Promise<WorkspaceCommandResult> {
    this.trackSender(sender)
    const resolved = await this.deps.validatePathAccess(sender.id, projectPath)
    await execFileAsync('git', ['init'], { cwd: resolved })

    const detectedInfo = await GitRepository.detect(resolved).unwrapOr(defaultGitInfo)
    const detectedProjectRoot = detectedInfo.repoRoot ?? resolved
    const projectRoot = await this.deps.validatePathAccess(sender.id, detectedProjectRoot)
    const info: GitInfo = {
      ...detectedInfo,
      repoRoot: detectedInfo.repoRoot ? projectRoot : detectedInfo.repoRoot,
      worktrees: detectedInfo.worktrees.map((worktree) =>
        worktree.path === detectedProjectRoot ? { ...worktree, path: projectRoot } : worktree,
      ),
    }
    this.updateGitStatusCache(sender.id, info)
    const projects = this.getProjects(sender.id)
    const projectIndex = projects.findIndex(
      (project) =>
        project.workspace.path === projectPath || this.projectKey(project) === projectPath,
    )

    if (projectIndex < 0) return this.result(sender.id, [], [])

    const existing = projects[projectIndex]
    const workspace = this.deps.workspaceStore.upsert({
      path: projectRoot,
      name: existing.workspace.name,
      isGitRepo: info.isGitRepo,
    })
    this.deps.workspaceStore.touch(workspace.id)

    const updated: ProjectSnapshot = {
      workspace,
      isGitRepo: info.isGitRepo,
      repoRoot: info.repoRoot,
      worktrees: info.worktrees,
    }
    projects.splice(projectIndex, 1, updated)
    this.projectsByWindow.set(sender.id, projects)

    if (projectRoot !== projectPath) {
      this.deps.windowManager.removeWorkspacePath(sender.id, projectPath)
      this.deps.windowManager.addWorkspacePath(sender.id, projectRoot)
    }

    const warnings: CommandWarning[] = []
    if (info.isGitRepo && info.repoRoot) {
      await this.startGitWatcher(sender, info.repoRoot, info, warnings)
    }

    this.deps.persistWindowConfigs()
    const result = this.result(sender.id, warnings, [])
    this.emitAppStateChanged(sender)
    return result
  }

  private async attachProjectSnapshot(
    sender: WebContents,
    requestedPath: string,
    options: AttachProjectOptions,
  ): Promise<boolean> {
    this.validateAttachPath(sender.id, requestedPath, requestedPath)
    const info = await GitRepository.detect(requestedPath).unwrapOr(defaultGitInfo)
    const projectPath = info.repoRoot ?? requestedPath
    this.validateAttachPath(sender.id, requestedPath, projectPath)

    const existingWindow = this.deps.windowManager.getWindowForPath(projectPath)
    if (existingWindow && existingWindow.webContents.id !== sender.id) {
      if (existingWindow.isMinimized()) existingWindow.restore()
      existingWindow.focus()
      return true
    }

    const projects = this.getProjects(sender.id)
    const existingProject = projects.find((project) => this.projectKey(project) === projectPath)
    if (existingProject) {
      if (options.selectIfEmpty && !this.selectedWorktreeByWindow.get(sender.id)) {
        const defaultWorktreePath = this.getDefaultWorktreePath(existingProject)
        if (defaultWorktreePath) {
          this.selectedWorktreeByWindow.set(sender.id, defaultWorktreePath)
          this.deps.windowManager.setActiveWorktree(sender.id, defaultWorktreePath)
          this.deps.persistWindowConfigs()
        }
      }
      return false
    }

    const workspace = this.deps.workspaceStore.upsert({
      path: projectPath,
      name: path.basename(projectPath),
      isGitRepo: info.isGitRepo,
    })
    this.deps.workspaceStore.touch(workspace.id)

    const project: ProjectSnapshot = {
      workspace,
      isGitRepo: info.isGitRepo,
      repoRoot: info.repoRoot,
      worktrees: info.worktrees,
    }

    this.projectsByWindow.set(sender.id, [...projects, project])
    this.updateGitStatusCache(sender.id, info)

    this.deps.windowManager.addWorkspacePath(sender.id, projectPath)
    this.deps.persistWindowConfigs()

    if (info.isGitRepo && info.repoRoot) {
      await this.startGitWatcher(sender, info.repoRoot, info, options.warnings)
    }

    this.collectRestoredLayouts(workspace, project, options.restoredLayouts)

    if (options.selectIfEmpty && !this.selectedWorktreeByWindow.get(sender.id)) {
      const defaultWorktreePath = this.getDefaultWorktreePath(project)
      if (defaultWorktreePath) {
        this.selectedWorktreeByWindow.set(sender.id, defaultWorktreePath)
        this.deps.windowManager.setActiveWorktree(sender.id, defaultWorktreePath)
        this.deps.persistWindowConfigs()
      }
    }
    return false
  }

  private async startGitWatcher(
    sender: WebContents,
    repoRoot: string,
    snapshot: GitInfo,
    warnings: CommandWarning[],
  ): Promise<void> {
    this.deps.windowManager.disposeGitWatcher(sender.id, repoRoot)

    const ws = this.deps.workspaceStore.getByPath(repoRoot)
    const workspaceId = ws?.id ?? null

    const watcher = new GitWatcher(
      repoRoot,
      (info, changes: GitRefreshFlags) => {
        if (workspaceId) {
          this.deps.workspaceStore.updateGitCache(workspaceId, {
            branch: info.branch,
            dirty: info.isDirty,
            aheadBehind: info.aheadBehind
              ? `${info.aheadBehind.ahead}/${info.aheadBehind.behind}`
              : null,
            worktreeCount: info.worktrees.length,
          })
        }
        this.updateProjectGitInfo(sender.id, repoRoot, info)
        this.updateGitStatusCache(sender.id, info)
        this.emitAppStateChanged(sender)
        if (!sender.isDestroyed()) {
          sender.send('git:changed', { ...info, repoRoot, changes })
        }
      },
      snapshot,
    )

    const startResult = await watcher.start()
    if (startResult.isErr()) {
      const message = gitErrorMessage(startResult.error)
      console.warn(message)
      warnings.push({
        code: 'git-watch-failed',
        message: `Git watcher failed for ${path.basename(repoRoot)} - status may not update`,
        paths: [repoRoot],
      })
    }
    this.deps.windowManager.setGitWatcher(sender.id, repoRoot, watcher)
  }

  private collectRestoredLayouts(
    workspace: WorkspaceRow,
    project: ProjectSnapshot,
    restoredLayouts: Array<{ worktreePath: string; layoutJson: string }>,
  ): void {
    const layouts = this.deps.layoutStore.getAll(workspace.id)
    if (layouts.length === 0) return

    const ownedPaths = this.getOwnedPaths(project)
    for (const entry of layouts) {
      if (ownedPaths.has(entry.worktree_path)) {
        restoredLayouts.push({ worktreePath: entry.worktree_path, layoutJson: entry.layout_json })
      } else {
        this.deps.layoutStore.delete(workspace.id, entry.worktree_path)
      }
    }
  }

  private result(
    webContentsId: number,
    warnings: CommandWarning[],
    restoredLayouts: Array<{ worktreePath: string; layoutJson: string }>,
  ): WorkspaceCommandResult {
    const snapshot = this.getSnapshot(webContentsId)
    return {
      ...snapshot,
      ...(restoredLayouts.length > 0 ? { restoredLayouts } : {}),
      warnings,
    }
  }

  private workspaceState(webContentsId: number): WorkspaceStateSnapshot {
    const selectedWorktreePath = this.selectedWorktreeByWindow.get(webContentsId) ?? null
    const project = selectedWorktreePath
      ? this.getProjectForWorktree(webContentsId, selectedWorktreePath)
      : null

    if (!project) {
      return {
        project: null,
        selectedWorktreePath,
        branch: null,
        isDirty: false,
        aheadBehind: null,
      }
    }

    const selectedWorktree = project.worktrees.find(
      (worktree) => worktree.path === selectedWorktreePath,
    )
    const status = selectedWorktreePath
      ? this.getWorktreeStatus(webContentsId, selectedWorktreePath)
      : null
    return {
      project,
      selectedWorktreePath,
      branch: status?.branch ?? selectedWorktree?.branch ?? null,
      isDirty: status?.isDirty ?? false,
      aheadBehind: status?.aheadBehind ?? null,
    }
  }

  private getProjects(webContentsId: number): ProjectSnapshot[] {
    return this.projectsByWindow.get(webContentsId) ?? []
  }

  private trackSender(sender: WebContents): void {
    if (this.trackedWebContents.has(sender.id)) return
    this.trackedWebContents.add(sender.id)
    sender.once('destroyed', () => {
      this.projectsByWindow.delete(sender.id)
      this.selectedWorktreeByWindow.delete(sender.id)
      this.statusByWorktreeByWindow.delete(sender.id)
      this.grantedAttachPathsByWindow.delete(sender.id)
      this.trackedWebContents.delete(sender.id)
    })
  }

  private getProjectForWorktree(
    webContentsId: number,
    worktreePath: string,
  ): ProjectSnapshot | null {
    return (
      this.getProjects(webContentsId).find((project) =>
        this.projectOwnsPath(project, worktreePath),
      ) ?? null
    )
  }

  private projectOwnsPath(project: ProjectSnapshot, worktreePath: string): boolean {
    if (project.workspace.path === worktreePath || project.repoRoot === worktreePath) return true
    return project.worktrees.some((worktree) => worktree.path === worktreePath)
  }

  private validateAttachPath(
    webContentsId: number,
    requestedPath: string,
    projectPath: string,
  ): void {
    const grants = this.grantedAttachPathsByWindow.get(webContentsId)
    if (grants?.has(requestedPath) || grants?.has(projectPath)) return

    const currentWindowPaths = this.deps.windowManager.getWorkspacePaths(webContentsId)
    if (currentWindowPaths.includes(requestedPath) || currentWindowPaths.includes(projectPath))
      return

    if (
      this.deps.workspaceStore.getByPath(requestedPath) ||
      this.deps.workspaceStore.getByPath(projectPath)
    ) {
      return
    }

    throw new Error('Project path was not selected through an allowed main-process flow')
  }

  private async refreshProjectForPath(webContentsId: number, worktreePath: string): Promise<void> {
    const info = await GitRepository.detect(worktreePath).unwrapOr(defaultGitInfo)
    if (!info.repoRoot) return
    this.updateProjectGitInfo(webContentsId, info.repoRoot, info)
    this.updateGitStatusCache(webContentsId, info)
  }

  private async refreshSelectedWorktreeStatus(
    webContentsId: number,
    worktreePath: string,
  ): Promise<void> {
    const project = this.getProjectForWorktree(webContentsId, worktreePath)
    if (!project?.isGitRepo) return

    const info = await GitRepository.detect(worktreePath).unwrapOr(defaultGitInfo)
    if (!info.isGitRepo) return

    const worktree = info.worktrees.find((candidate) => candidate.path === worktreePath)
    this.setWorktreeStatus(webContentsId, worktreePath, {
      branch: worktree?.branch ?? info.branch,
      isDirty: info.isDirty,
      aheadBehind: info.aheadBehind,
    })
  }

  private updateProjectGitInfo(webContentsId: number, repoRoot: string, info: GitInfo): void {
    const projects = this.getProjects(webContentsId)
    const detectedPaths = new Set(info.worktrees.map((worktree) => worktree.path))
    if (info.repoRoot) detectedPaths.add(info.repoRoot)

    const index = projects.findIndex((project) => {
      if (project.repoRoot === repoRoot || project.workspace.path === repoRoot) return true
      if (project.repoRoot && detectedPaths.has(project.repoRoot)) return true
      if (detectedPaths.has(project.workspace.path)) return true
      return project.worktrees.some((worktree) => detectedPaths.has(worktree.path))
    })
    if (index < 0) return

    const current = projects[index]
    projects.splice(index, 1, {
      ...current,
      isGitRepo: info.isGitRepo,
      repoRoot: info.repoRoot ?? repoRoot,
      worktrees: info.worktrees,
    })
    this.projectsByWindow.set(webContentsId, projects)
  }

  private updateGitStatusCache(webContentsId: number, info: GitInfo): void {
    const mainWorktreePath = info.worktrees.find((worktree) => worktree.isMain)?.path
    const repoStatus: WorktreeStatusSnapshot = {
      branch: info.branch,
      isDirty: info.isDirty,
      aheadBehind: info.aheadBehind,
    }

    if (info.repoRoot) this.setWorktreeStatus(webContentsId, info.repoRoot, repoStatus)
    if (mainWorktreePath) this.setWorktreeStatus(webContentsId, mainWorktreePath, repoStatus)

    for (const worktree of info.worktrees) {
      if (worktree.path === mainWorktreePath) continue
      const existing = this.getWorktreeStatus(webContentsId, worktree.path)
      this.setWorktreeStatus(webContentsId, worktree.path, {
        branch: worktree.branch ?? existing?.branch ?? null,
        isDirty: existing?.isDirty ?? false,
        aheadBehind: existing?.aheadBehind ?? null,
      })
    }
  }

  private setWorktreeStatus(
    webContentsId: number,
    worktreePath: string,
    status: WorktreeStatusSnapshot,
  ): void {
    let statuses = this.statusByWorktreeByWindow.get(webContentsId)
    if (!statuses) {
      statuses = new Map()
      this.statusByWorktreeByWindow.set(webContentsId, statuses)
    }
    statuses.set(worktreePath, status)
  }

  private getWorktreeStatus(
    webContentsId: number,
    worktreePath: string,
  ): WorktreeStatusSnapshot | null {
    return this.statusByWorktreeByWindow.get(webContentsId)?.get(worktreePath) ?? null
  }

  private clearProjectStatus(webContentsId: number, project: ProjectSnapshot): void {
    const statuses = this.statusByWorktreeByWindow.get(webContentsId)
    if (!statuses) return
    for (const ownedPath of this.getOwnedPaths(project)) {
      statuses.delete(ownedPath)
    }
    if (statuses.size === 0) this.statusByWorktreeByWindow.delete(webContentsId)
  }

  private getDefaultWorktreePath(project: ProjectSnapshot | undefined): string | undefined {
    if (!project) return undefined
    if (!project.isGitRepo) return project.workspace.path
    const main = project.worktrees.find((worktree) => worktree.isMain)
    return main?.path ?? project.repoRoot ?? project.workspace.path
  }

  private getOwnedPaths(project: ProjectSnapshot): Set<string> {
    const ownedPaths = new Set<string>([project.workspace.path])
    if (project.repoRoot) ownedPaths.add(project.repoRoot)
    for (const worktree of project.worktrees) ownedPaths.add(worktree.path)
    return ownedPaths
  }

  private projectKey(project: ProjectSnapshot): string {
    return project.repoRoot ?? project.workspace.path
  }

  private emitAppStateChanged(sender: WebContents): void {
    if (sender.isDestroyed()) return
    this.deps.emitAppStateChanged(sender)
  }

  private stalePathsWarning(removedPaths: string[]): CommandWarning {
    const basenames = removedPaths.map((removedPath) => path.basename(removedPath))
    const hasCollision = new Set(basenames).size !== basenames.length
    const labels = hasCollision
      ? removedPaths.map((removedPath) => {
          const parent = path.basename(path.dirname(removedPath))
          const name = path.basename(removedPath)
          return parent ? `${parent}/${name}` : name || removedPath
        })
      : basenames
    const plural = removedPaths.length === 1 ? 'project' : 'projects'
    return {
      code: 'stale-paths-removed',
      message: `Removed ${removedPaths.length} stale ${plural} (folder missing): ${labels.join(', ')}`,
      paths: removedPaths,
    }
  }
}
