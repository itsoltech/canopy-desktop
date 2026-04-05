<script lang="ts">
  import { Check, X } from '@lucide/svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import {
    getRepoConfig,
    getHasCredentials,
    getCredentialsInfo,
    saveRepoConfig,
    loadRepoConfig,
  } from '../../lib/stores/taskTracker.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'

  interface Props {
    repoRoot: string
  }

  let { repoRoot }: Props = $props()

  let config = $derived(getRepoConfig())
  let credentialsPresent = $derived(getHasCredentials())
  let credentialsInfo = $derived(getCredentialsInfo())

  let provider = $state<'jira' | 'youtrack'>('jira')
  let baseUrl = $state('')
  let username = $state('')
  let tokenInput = $state('')
  let testing = $state(false)
  let testResult = $state<'success' | 'fail' | ''>('')
  let initialized = $state(false)

  $effect(() => {
    if (config && !initialized) {
      provider = config.tracker.provider
      baseUrl = config.tracker.baseUrl
      initialized = true
    }
  })

  $effect(() => {
    if (credentialsInfo && !username) {
      username = credentialsInfo.username ?? ''
    }
  })

  async function testConnection(): Promise<void> {
    testing = true
    testResult = ''
    try {
      await window.api.taskTrackerTestNewConnection({
        provider,
        name: `${provider}:${baseUrl}`,
        baseUrl: baseUrl.replace(/\/$/, ''),
        projectKey: '',
        username: username || undefined,
        token: tokenInput,
      })
      testResult = 'success'
    } catch {
      testResult = 'fail'
    } finally {
      testing = false
    }
  }

  async function saveConfig(): Promise<void> {
    if (!config) return
    try {
      const updated = {
        ...config,
        tracker: {
          provider,
          baseUrl: baseUrl.replace(/\/$/, ''),
        },
      }
      await saveRepoConfig(repoRoot, updated)

      if (tokenInput) {
        await window.api.keychainSetCredentials(
          provider,
          baseUrl.replace(/\/$/, ''),
          tokenInput,
          username || undefined,
        )
        tokenInput = ''
        await loadRepoConfig(repoRoot)
      }

      addToast('Tracker configuration saved')
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to save configuration')
    }
  }

  async function deleteCredentials(): Promise<void> {
    try {
      await window.api.keychainDeleteCredentials(provider, baseUrl.replace(/\/$/, ''))
      await loadRepoConfig(repoRoot)
      addToast('Credentials removed')
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to remove credentials')
    }
  }
</script>

{#if config}
  <div class="subsection">
    <h4 class="subsection-title">Connection</h4>

    <div class="field">
      <label class="field-label">Provider</label>
      <CustomSelect
        value={provider}
        options={[
          { value: 'jira', label: 'Jira' },
          { value: 'youtrack', label: 'YouTrack' },
        ]}
        onchange={(v) => (provider = v as 'jira' | 'youtrack')}
      />
    </div>

    <div class="field">
      <label class="field-label">Base URL</label>
      <input
        class="text-input"
        bind:value={baseUrl}
        placeholder="https://company.atlassian.net"
        spellcheck="false"
      />
    </div>
  </div>

  <div class="subsection">
    <h4 class="subsection-title">Credentials</h4>
    <span class="field-hint">Stored locally per user — never committed to the repository.</span>

    {#if provider === 'jira'}
      <div class="field">
        <label class="field-label">Email</label>
        <input
          class="text-input"
          bind:value={username}
          placeholder="user@company.com"
          spellcheck="false"
        />
      </div>
    {/if}

    <div class="field">
      <label class="field-label"
        >API Token
        {#if provider === 'jira'}
          <button
            class="token-link"
            onclick={() =>
              window.api.openExternal(
                'https://id.atlassian.com/manage-profile/security/api-tokens',
              )}
          >
            Generate token
          </button>
        {:else if provider === 'youtrack'}
          <button
            class="token-link"
            onclick={() => {
              const url = baseUrl
                ? `${baseUrl.replace(/\/$/, '')}/hub/tokens`
                : 'https://youtrack.jetbrains.com/hub/tokens'
              window.api.openExternal(url)
            }}
          >
            Generate token
          </button>
        {/if}
      </label>
      <input
        class="text-input"
        type="password"
        bind:value={tokenInput}
        placeholder={credentialsPresent ? '••••••••' : 'Enter token'}
        autocomplete="off"
      />
    </div>

    {#if credentialsPresent}
      <div class="status-row">
        <span class="status-ok"
          ><Check size={12} /> Authenticated{credentialsInfo?.username
            ? ` as ${credentialsInfo.username}`
            : ''}</span
        >
        <button class="remove-btn" onclick={deleteCredentials}>Remove</button>
      </div>
    {:else}
      <span class="status-missing">Credentials required</span>
    {/if}
  </div>

  <div class="test-row" aria-live="polite">
    {#if testResult === 'success'}
      <span class="status-ok"><Check size={14} /> Connection OK</span>
    {:else if testResult === 'fail'}
      <span class="status-fail"><X size={14} /> Connection failed</span>
    {/if}
  </div>

  <div class="form-actions">
    <button
      class="btn btn-secondary"
      onclick={testConnection}
      disabled={testing || !baseUrl || (!tokenInput && !credentialsPresent)}
    >
      {#if testing}Testing...{:else}Test Connection{/if}
    </button>
    <button class="btn btn-primary" onclick={saveConfig} disabled={!baseUrl}> Save </button>
  </div>
{:else}
  <p class="field-hint">No tracker configured for this repository.</p>
{/if}

<style>
  .subsection {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .subsection-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--c-text-muted);
    margin: 0;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .field-label {
    font-size: 12px;
    font-weight: 500;
    color: var(--c-text-secondary);
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .token-link {
    font-size: 11px;
    font-weight: 400;
    color: var(--c-accent-text);
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    font-family: inherit;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .token-link:hover {
    color: var(--c-accent);
  }

  .field-hint {
    font-size: 11px;
    color: var(--c-text-faint);
  }

  .text-input {
    padding: 6px 10px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-hover);
    color: var(--c-text);
    font-size: 13px;
    font-family: inherit;
    outline: none;
  }

  .text-input:focus {
    border-color: var(--c-focus-ring);
  }

  .status-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .status-ok {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--c-success);
  }

  .status-missing {
    font-size: 12px;
    color: var(--c-warning-text);
  }

  .status-fail {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--c-danger-text);
  }

  .remove-btn {
    padding: 2px 8px;
    border: none;
    border-radius: 4px;
    background: var(--c-danger-bg);
    color: var(--c-danger-text);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    flex-shrink: 0;
  }

  .test-row {
    min-height: 20px;
  }

  .form-actions {
    display: flex;
    gap: 8px;
  }

  .btn {
    padding: 6px 14px;
    border: none;
    border-radius: 6px;
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .btn-primary {
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
  }

  .btn-primary:hover:not(:disabled) {
    background: var(--c-accent-bg-hover);
  }

  .btn-secondary {
    background: var(--c-active);
    color: var(--c-text);
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--c-hover-strong);
  }
</style>
