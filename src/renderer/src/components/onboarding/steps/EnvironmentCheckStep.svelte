<script lang="ts">
  import { onMount } from 'svelte'
  import { CircleCheck, CircleAlert, RefreshCw, Copy, Check } from '@lucide/svelte'
  import { onboardingState } from '../../../lib/stores/onboarding.svelte'

  interface DepStatus {
    found: boolean
    path?: string
  }

  interface InstallOption {
    label: string
    cmd: string
  }

  interface ToolInfo {
    label: string
    install: (platform: string) => InstallOption[]
  }

  const toolMeta: Record<string, ToolInfo> = {
    claude: {
      label: 'Claude Code CLI',
      install: (platform) => {
        if (platform === 'win32') {
          return [
            { label: 'PowerShell', cmd: 'irm https://claude.ai/install.ps1 | iex' },
            { label: 'npm', cmd: 'npm install -g @anthropic-ai/claude-code' },
          ]
        }
        return [
          { label: 'Script', cmd: 'curl -fsSL https://claude.ai/install.sh | bash' },
          { label: 'Homebrew', cmd: 'brew install --cask claude-code' },
          { label: 'npm', cmd: 'npm install -g @anthropic-ai/claude-code' },
        ]
      },
    },
    codex: {
      label: 'Codex',
      install: (platform) => {
        if (platform === 'win32') {
          return [{ label: 'npm', cmd: 'npm install -g @openai/codex' }]
        }
        return [
          { label: 'Homebrew', cmd: 'brew install --cask codex' },
          { label: 'npm', cmd: 'npm install -g @openai/codex' },
        ]
      },
    },
    gemini: {
      label: 'Gemini CLI',
      install: (platform) => {
        if (platform === 'win32') {
          return [{ label: 'npm', cmd: 'npm install -g @google/gemini-cli' }]
        }
        return [
          { label: 'Homebrew', cmd: 'brew install gemini-cli' },
          { label: 'npm', cmd: 'npm install -g @google/gemini-cli' },
        ]
      },
    },
    git: {
      label: 'Git',
      install: (platform) => {
        if (platform === 'darwin') {
          return [
            { label: 'Xcode tools', cmd: 'xcode-select --install' },
            { label: 'Homebrew', cmd: 'brew install git' },
          ]
        }
        if (platform === 'win32') {
          return [{ label: 'winget', cmd: 'winget install Git.Git' }]
        }
        if (platform === 'linux') {
          return [{ label: 'apt', cmd: 'sudo apt install git' }]
        }
        return [{ label: 'Download', cmd: 'https://git-scm.com/download' }]
      },
    },
  }

  let results: Record<string, DepStatus> = $state({})
  let platform = $state('')
  let checking = $state(true)
  let copiedCmd: string | null = $state(null)
  let copyTimer: ReturnType<typeof setTimeout> | null = null

  let checkedTools = $derived([...onboardingState.selectedTools, 'git'])

  async function runCheck(): Promise<void> {
    checking = true
    const data = await window.api.checkDependencies([...onboardingState.selectedTools])
    results = data.results
    platform = data.platform
    checking = false
  }

  async function copyCommand(cmd: string): Promise<void> {
    await navigator.clipboard.writeText(cmd)
    copiedCmd = cmd
    if (copyTimer) clearTimeout(copyTimer)
    copyTimer = setTimeout(() => {
      copiedCmd = null
      copyTimer = null
    }, 2000)
  }

  onMount(() => {
    runCheck()
    return () => {
      if (copyTimer) clearTimeout(copyTimer)
    }
  })

  let allFound = $derived(checkedTools.every((id) => results[id]?.found))
  let missingCount = $derived(checkedTools.filter((id) => !results[id]?.found).length)
  let hasResults = $derived(Object.keys(results).length > 0)
</script>

<div class="step">
  <h2 class="title">Environment check</h2>
  <p class="description">
    {#if checking}Checking installed tools...{:else if allFound}All selected tools are installed.{:else}{missingCount}
      missing -- install now or skip and do it later.{/if}
  </p>

  {#if hasResults}
    <div class="checklist">
      {#each checkedTools as toolId (toolId)}
        {@const status = results[toolId]}
        {@const meta = toolMeta[toolId]}
        {#if meta}
          <div class="check-row">
            <div class="check-status">
              {#if status?.found}
                <CircleCheck size={20} color="var(--c-success)" />
              {:else}
                <CircleAlert size={20} color="var(--c-warning)" />
              {/if}
            </div>
            <div class="check-info">
              <span class="check-label">{meta.label}</span>
              {#if status?.found}
                <span class="check-detail">{status.path}</span>
              {:else}
                <span class="check-detail">Not found in PATH</span>
                <div class="install-options">
                  {#each meta.install(platform) as opt (opt.cmd)}
                    <div class="install-row">
                      <span class="install-method">{opt.label}</span>
                      {#if opt.cmd.startsWith('http')}
                        <button
                          class="install-link"
                          onclick={() => window.api.openExternal(opt.cmd)}
                        >
                          {opt.cmd}
                        </button>
                      {:else}
                        <code class="install-cmd">{opt.cmd}</code>
                        <button
                          class="copy-btn"
                          onclick={() => copyCommand(opt.cmd)}
                          title="Copy to clipboard"
                          aria-label="Copy install command"
                        >
                          {#if copiedCmd === opt.cmd}
                            <Check size={14} />
                          {:else}
                            <Copy size={14} />
                          {/if}
                        </button>
                      {/if}
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          </div>
        {/if}
      {/each}
    </div>

    {#if !allFound}
      <button class="recheck-btn" onclick={runCheck} disabled={checking}>
        <RefreshCw size={14} class={checking ? 'spin' : ''} />
        Re-check
      </button>
    {/if}
  {:else}
    <div class="loading">
      <RefreshCw size={20} class="spin" color="var(--c-text-faint)" />
    </div>
  {/if}
</div>

<style>
  .step {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 16px;
  }

  .title {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    color: var(--c-text);
  }

  .description {
    margin: 0;
    font-size: 13px;
    color: var(--c-text-secondary);
    max-width: 380px;
    line-height: 1.5;
  }

  .checklist {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    max-width: 460px;
    text-align: left;
  }

  .check-row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 8px;
  }

  .check-status {
    flex-shrink: 0;
    padding-top: 1px;
  }

  .check-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }

  .check-label {
    font-size: 13px;
    font-weight: 500;
    color: var(--c-text);
  }

  .check-detail {
    font-size: 11px;
    color: var(--c-text-muted);
    font-family: monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .install-options {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-top: 6px;
  }

  .install-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .install-method {
    flex-shrink: 0;
    width: 56px;
    font-size: 10px;
    font-weight: 500;
    color: var(--c-text-faint);
    text-transform: uppercase;
    letter-spacing: 0.3px;
  }

  .install-cmd {
    flex: 1;
    min-width: 0;
    font-size: 11px;
    font-family: monospace;
    padding: 4px 8px;
    background: var(--c-hover);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    user-select: all;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .install-link {
    font-size: 11px;
    color: var(--c-accent);
    background: none;
    border: none;
    padding: 0;
    font-family: monospace;
    cursor: pointer;
    text-decoration: none;
  }

  .install-link:hover {
    text-decoration: underline;
  }

  .copy-btn {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    padding: 0;
    border: 1px solid var(--c-border);
    border-radius: 4px;
    background: var(--c-hover);
    color: var(--c-text-secondary);
    cursor: pointer;
    transition: background 0.1s;
  }

  .copy-btn:hover {
    background: var(--c-active);
  }

  .recheck-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 16px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: transparent;
    color: var(--c-text-secondary);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    transition: background 0.1s;
  }

  .recheck-btn:hover:not(:disabled) {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .recheck-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .loading {
    padding: 24px;
    display: flex;
    justify-content: center;
  }

  .recheck-btn :global(.spin),
  .loading :global(.spin) {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }
</style>
