import { test, expect, type ElectronApplication, type Page, _electron } from '@playwright/test'
import { existsSync, realpathSync } from 'fs'
import { chmod, mkdir, mkdtemp, readFile, rm, writeFile } from 'fs/promises'
import { tmpdir } from 'os'
import { delimiter, join, resolve } from 'path'

const appDir = resolve(__dirname, '..')

const agentCases = [
  {
    toolId: 'claude',
    sessionId: 'claude-real-session-e2e',
    expectedResumeArgs: ' --resume claude-real-session-e2e',
  },
  {
    toolId: 'codex',
    sessionId: 'codex-real-session-e2e',
    expectedResumeArgs: 'resume codex-real-session-e2e',
  },
] as const

interface AppStateSnapshot {
  workspace: {
    projects: Array<{ workspace: { path: string } }>
  }
  tabs: {
    tabsByWorktree: Record<string, unknown[]>
  }
}

interface CanopyWindow extends Window {
  api: {
    getAppState: () => Promise<AppStateSnapshot>
    perfOpenProject?: (path: string) => Promise<void>
    tabOpenTool?: (toolId: string, worktreePath: string) => Promise<unknown>
    tabSaveCurrentLayout?: (worktreePath: string) => Promise<void>
  }
}

async function launchApp(userDataDir: string, fakeBinDir: string): Promise<ElectronApplication> {
  const fakeShell = join(fakeBinDir, 'canopy-login-env-shell')
  return _electron.launch({
    args: [resolve(appDir, 'out/main/index.js')],
    env: {
      ...process.env,
      NODE_ENV: 'test',
      CANOPY_E2E: '1',
      CANOPY_PERF: '1',
      CANOPY_TEST_USER_DATA: userDataDir,
      PATH: `${fakeBinDir}${delimiter}${process.env.PATH ?? ''}`,
      ...(process.platform === 'win32' ? {} : { SHELL: fakeShell }),
    },
  })
}

async function closeApp(app: ElectronApplication | null): Promise<void> {
  if (!app) return
  const proc = app.process()
  if (proc.exitCode !== null || proc.signalCode !== null) return

  await Promise.race([
    app.close().catch(() => {}),
    new Promise<void>((resolve) =>
      setTimeout(() => {
        if (proc.pid) {
          try {
            process.kill(proc.pid)
          } catch {
            // Process already exited.
          }
        }
        resolve()
      }, 5_000),
    ),
  ])
}

async function waitForExit(app: ElectronApplication, label: string): Promise<void> {
  const proc = app.process()
  if (proc.exitCode !== null || proc.signalCode !== null) return
  await Promise.race([
    new Promise<void>((resolve) => proc.once('exit', () => resolve())),
    new Promise<void>((_resolve, reject) =>
      setTimeout(() => reject(new Error(`${label} did not exit`)), 10_000),
    ),
  ])
}

async function quitApp(app: ElectronApplication, label: string): Promise<void> {
  await app.evaluate(({ app, dialog }) => {
    dialog.showMessageBox = async () => ({ response: 0, checkboxChecked: false })
    app.quit()
  })
  await waitForExit(app, label)
}

async function waitForLineCount(
  filePath: string,
  expectedCount: number,
  label: string,
): Promise<string[]> {
  const startedAt = Date.now()
  let lines: string[] = []

  while (Date.now() - startedAt < 8_000) {
    if (existsSync(filePath)) {
      const content = await readFile(filePath, 'utf-8')
      lines = content
        .split('\n')
        .map((line) => line.trim())
        .filter(Boolean)
      if (lines.length >= expectedCount) return lines
    }
    await new Promise((resolve) => setTimeout(resolve, 100))
  }

  throw new Error(`Expected ${expectedCount} ${label} invocation(s), got ${lines.length}`)
}

async function waitForFile(filePath: string): Promise<void> {
  const startedAt = Date.now()
  while (Date.now() - startedAt < 8_000) {
    if (existsSync(filePath)) return
    await new Promise((resolve) => setTimeout(resolve, 100))
  }
  throw new Error(`Timed out waiting for ${filePath}`)
}

async function getAppState(page: Page): Promise<AppStateSnapshot> {
  return page.evaluate(() => (window as unknown as CanopyWindow).api.getAppState())
}

async function waitForProjectCount(page: Page, expectedCount: number): Promise<void> {
  const startedAt = Date.now()
  let lastCount = 0

  while (Date.now() - startedAt < 8_000) {
    const state = await getAppState(page)
    lastCount = state.workspace.projects.length
    if (lastCount === expectedCount) return
    await page.waitForTimeout(100)
  }

  throw new Error(`Expected ${expectedCount} project(s), got ${lastCount}`)
}

async function createFakeAgent(
  fakeBinDir: string,
  binaryName: string,
  hookSessionId: string,
  argsLogPath: string,
  hookMarkerPath: string,
): Promise<void> {
  await mkdir(fakeBinDir, { recursive: true })

  if (process.platform !== 'win32') {
    const fakeShellPath = join(fakeBinDir, 'canopy-login-env-shell')
    await writeFile(
      fakeShellPath,
      `#!/bin/sh
exec /usr/bin/env -0
`,
      'utf-8',
    )
    await chmod(fakeShellPath, 0o755)
  }

  const shPath = join(fakeBinDir, binaryName)
  await writeFile(
    shPath,
    `#!/bin/sh
printf '%s\\n' "$*" >> "$FAKE_CODEX_ARGS_LOG"
if [ -n "$CANOPY_HOOK_PORT" ]; then
  printf '{"hook_event_name":"SessionStart","session_id":"${hookSessionId}"}' | curl -s -X POST "http://127.0.0.1:\${CANOPY_HOOK_PORT}\${CANOPY_HOOK_PATH:-}/hook" -H "Content-Type: application/json" -H "X-Canopy-Auth: \${CANOPY_HOOK_TOKEN}" --data-binary @- >/dev/null 2>&1
fi
printf 'sent' > "$FAKE_CODEX_HOOK_MARKER"
sleep 300
`,
    'utf-8',
  )
  await chmod(shPath, 0o755)

  await writeFile(
    join(fakeBinDir, `${binaryName}.cmd`),
    `@echo off
echo %*>>"%FAKE_CODEX_ARGS_LOG%"
powershell -NoProfile -Command "$body = '{\\"hook_event_name\\":\\"SessionStart\\",\\"session_id\\":\\"${hookSessionId}\\"}'; Invoke-RestMethod -Method Post -Uri ('http://127.0.0.1:' + $env:CANOPY_HOOK_PORT + $env:CANOPY_HOOK_PATH + '/hook') -Headers @{'Content-Type'='application/json'; 'X-Canopy-Auth'=$env:CANOPY_HOOK_TOKEN} -Body $body | Out-Null" >NUL 2>NUL
echo sent>"%FAKE_CODEX_HOOK_MARKER%"
timeout /t 300 >NUL
`,
    'utf-8',
  )

  process.env.FAKE_CODEX_ARGS_LOG = argsLogPath
  process.env.FAKE_CODEX_HOOK_MARKER = hookMarkerPath
}

for (const agentCase of agentCases) {
  test(`persists real ${agentCase.toolId} hook session id for resume`, async () => {
    const userDataDir = await mkdtemp(join(tmpdir(), 'canopy-e2e-agent-resume-data-'))
    const rawProject = await mkdtemp(join(tmpdir(), 'canopy-e2e-agent-resume-project-'))
    const projectPath = realpathSync(rawProject)
    const fakeBinDir = await mkdtemp(join(tmpdir(), 'canopy-e2e-fake-bin-'))
    const argsLogPath = join(fakeBinDir, `${agentCase.toolId}-args.log`)
    const hookMarkerPath = join(fakeBinDir, `${agentCase.toolId}-hook-sent`)
    let app: ElectronApplication | null = null

    try {
      await writeFile(join(rawProject, 'README.md'), 'Project\n')
      await createFakeAgent(
        fakeBinDir,
        agentCase.toolId,
        agentCase.sessionId,
        argsLogPath,
        hookMarkerPath,
      )

      app = await launchApp(userDataDir, fakeBinDir)
      let page = await app.firstWindow()
      await page.waitForLoadState('domcontentloaded')
      await page.waitForFunction(() => {
        const api = (window as unknown as CanopyWindow).api
        return Boolean(api?.perfOpenProject && api?.tabOpenTool && api?.tabSaveCurrentLayout)
      })

      await page.evaluate(
        (path) => (window as unknown as CanopyWindow).api.perfOpenProject?.(path),
        projectPath,
      )
      await waitForProjectCount(page, 1)
      await page.evaluate(
        ({ path, toolId }) => (window as unknown as CanopyWindow).api.tabOpenTool?.(toolId, path),
        { path: projectPath, toolId: agentCase.toolId },
      )

      await waitForFile(hookMarkerPath)
      await page.evaluate(
        (path) => (window as unknown as CanopyWindow).api.tabSaveCurrentLayout?.(path),
        projectPath,
      )
      await quitApp(app, `initial ${agentCase.toolId} app`)
      app = null

      const firstInvocations = await waitForLineCount(argsLogPath, 1, agentCase.toolId)
      expect(firstInvocations[0]).not.toContain(agentCase.sessionId)

      app = await launchApp(userDataDir, fakeBinDir)
      page = await app.firstWindow()
      await page.waitForLoadState('domcontentloaded')
      await waitForProjectCount(page, 1)

      const invocations = await waitForLineCount(argsLogPath, 2, agentCase.toolId)
      expect(` ${invocations[1]}`).toContain(agentCase.expectedResumeArgs)

      await quitApp(app, `restored ${agentCase.toolId} app`)
      app = null
    } finally {
      delete process.env.FAKE_CODEX_ARGS_LOG
      delete process.env.FAKE_CODEX_HOOK_MARKER
      await closeApp(app)
      await rm(userDataDir, { recursive: true, force: true })
      await rm(rawProject, { recursive: true, force: true })
      await rm(fakeBinDir, { recursive: true, force: true })
    }
  })
}
