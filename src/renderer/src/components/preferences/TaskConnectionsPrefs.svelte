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

<div class="section">
  <h3 class="section-title">Tracker Connection</h3>
  <p class="section-desc">Configure the task tracker for this repository.</p>

  {#if config}
    <div class="form">
      <div class="form-row">
        <label class="form-label">Provider</label>
        <CustomSelect
          value={provider}
          options={[
            { value: 'jira', label: 'Jira' },
            { value: 'youtrack', label: 'YouTrack' },
          ]}
          onchange={(v) => (provider = v as 'jira' | 'youtrack')}
          maxWidth="none"
        />
      </div>
      <div class="form-row">
        <label class="form-label">Base URL</label>
        <input
          class="form-input"
          bind:value={baseUrl}
          placeholder="https://company.atlassian.net"
        />
      </div>

      <div class="creds-section">
        <h4 class="subsection-title">Credentials</h4>
        <p class="section-desc">Stored locally per user — never committed to the repository.</p>
        {#if provider === 'jira'}
          <div class="form-row">
            <label class="form-label">Email</label>
            <input class="form-input" bind:value={username} placeholder="user@company.com" />
          </div>
        {/if}
        <div class="form-row">
          <label class="form-label">API Token</label>
          <input
            class="form-input"
            type="password"
            bind:value={tokenInput}
            placeholder={credentialsPresent ? '••••••••' : 'Enter token'}
          />
        </div>
        <div class="token-status">
          {#if credentialsPresent}
            <span class="status-ok"
              ><Check size={12} /> Authenticated{credentialsInfo?.username
                ? ` as ${credentialsInfo.username}`
                : ''}</span
            >
            <button class="btn-link" onclick={deleteCredentials}>Remove</button>
          {:else}
            <span class="status-missing">Credentials required</span>
          {/if}
        </div>
      </div>

      <div class="test-result" aria-live="polite">
        {#if testResult === 'success'}
          <span class="test-ok"><Check size={14} /> Connection OK</span>
        {:else if testResult === 'fail'}
          <span class="test-fail"><X size={14} /> Connection failed</span>
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
    </div>
  {:else}
    <p class="no-config">No tracker configured for this repository.</p>
  {/if}
</div>

<style>
  .section {
    margin-bottom: 24px;
  }
  .section-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--c-text);
    margin: 0 0 4px;
  }
  .section-desc {
    font-size: 12px;
    color: var(--c-text-muted);
    margin: 0 0 12px;
  }
  .subsection-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--c-text-secondary);
    margin: 0 0 4px;
  }
  .form {
    padding: 12px;
    border: 1px solid var(--c-border);
    border-radius: 8px;
    background: var(--c-bg-input);
  }
  .form-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }
  .form-label {
    font-size: 12px;
    color: var(--c-text-secondary);
    width: 90px;
    flex-shrink: 0;
  }
  .form-input {
    flex: 1;
    padding: 5px 8px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-bg-input);
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
    outline: none;
  }
  .form-input:focus {
    border-color: var(--c-focus-ring);
  }
  .creds-section {
    margin: 12px 0 8px;
    padding-top: 12px;
    border-top: 1px solid var(--c-border-subtle);
  }
  .token-status {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 8px;
    padding-left: 98px;
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
  .btn-link {
    background: none;
    border: none;
    color: var(--c-text-muted);
    font-size: 11px;
    cursor: pointer;
    text-decoration: underline;
    padding: 0;
  }
  .btn-link:hover {
    color: var(--c-danger-text);
  }
  .form-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
  }
  .btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    border: none;
    border-radius: 6px;
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    transition: background 0.1s;
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
    color: var(--c-text-secondary);
  }
  .btn-secondary:hover:not(:disabled) {
    background: var(--c-hover-strong);
  }
  .test-result {
    min-height: 20px;
    margin-bottom: 4px;
  }
  .test-ok,
  .test-fail {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
  }
  .test-ok {
    color: var(--c-success);
  }
  .test-fail {
    color: var(--c-danger-text);
  }
  .no-config {
    font-size: 12px;
    color: var(--c-text-muted);
  }
</style>
