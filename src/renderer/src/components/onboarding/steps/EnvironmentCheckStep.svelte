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

<div class="flex flex-col items-center text-center gap-4">
  <h2 class="m-0 text-lg font-semibold text-text">Environment check</h2>
  <p class="m-0 text-md text-text-secondary max-w-[380px] leading-normal">
    {#if checking}Checking installed tools...{:else if allFound}All selected tools are installed.{:else}{missingCount}
      missing -- install now or skip and do it later.{/if}
  </p>

  {#if hasResults}
    <div class="flex flex-col gap-0.5 w-full max-w-[460px] text-left">
      {#each checkedTools as toolId (toolId)}
        {@const status = results[toolId]}
        {@const meta = toolMeta[toolId]}
        {#if meta}
          <div class="flex items-start gap-2.5 px-3 py-2.5 rounded-xl">
            <div class="flex-shrink-0 pt-px">
              {#if status?.found}
                <CircleCheck size={20} color="var(--color-success)" />
              {:else}
                <CircleAlert size={20} color="var(--color-warning)" />
              {/if}
            </div>
            <div class="flex flex-col gap-0.5 min-w-0 flex-1">
              <span class="text-md font-medium text-text">{meta.label}</span>
              {#if status?.found}
                <span
                  class="text-xs text-text-muted font-mono overflow-hidden text-ellipsis whitespace-nowrap"
                  >{status.path}</span
                >
              {:else}
                <span class="text-xs text-text-muted font-mono">Not found in PATH</span>
                <div class="flex flex-col gap-1 mt-1.5">
                  {#each meta.install(platform) as opt (opt.cmd)}
                    <div class="flex items-center gap-1.5">
                      <span
                        class="flex-shrink-0 w-14 text-2xs font-medium text-text-faint uppercase tracking-[0.3px]"
                        >{opt.label}</span
                      >
                      {#if opt.cmd.startsWith('http')}
                        <button
                          class="text-xs text-accent bg-transparent border-0 p-0 font-mono cursor-pointer no-underline hover:underline"
                          onclick={() => window.api.openExternal(opt.cmd)}
                        >
                          {opt.cmd}
                        </button>
                      {:else}
                        <code
                          class="flex-1 min-w-0 text-xs font-mono px-2 py-1 bg-hover border border-border rounded-md text-text select-all overflow-hidden text-ellipsis whitespace-nowrap"
                          >{opt.cmd}</code
                        >
                        <button
                          class="flex-shrink-0 flex items-center justify-center w-[26px] h-[26px] p-0 border border-border rounded-md bg-hover text-text-secondary cursor-pointer transition-colors duration-fast hover:bg-active"
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
      <button
        class="flex items-center gap-1.5 px-4 py-1.5 border border-border rounded-lg bg-transparent text-text-secondary text-sm font-inherit cursor-pointer transition-colors duration-fast enabled:hover:bg-hover enabled:hover:text-text disabled:opacity-50 disabled:cursor-default"
        onclick={runCheck}
        disabled={checking}
      >
        <RefreshCw size={14} class={checking ? 'animate-spin' : ''} />
        Re-check
      </button>
    {/if}
  {:else}
    <div class="p-6 flex justify-center">
      <RefreshCw size={20} class="animate-spin" color="var(--color-text-faint)" />
    </div>
  {/if}
</div>
