<script lang="ts">
  import { Check, X } from '@lucide/svelte'
  import { match } from 'ts-pattern'
  import CustomSelect from '../../shared/CustomSelect.svelte'

  type Provider = 'jira' | 'youtrack' | 'github'

  let {
    provider = $bindable(),
    baseUrl = $bindable(),
    projectKey = $bindable(),
    username = $bindable(),
    token = $bindable(),
    isNew,
    hasExistingToken,
    testing,
    testResult,
    onCancel,
    onTest,
    onSave,
  }: {
    provider: Provider
    baseUrl: string
    projectKey: string
    username: string
    token: string
    isNew: boolean
    hasExistingToken: boolean
    testing: boolean
    testResult: 'success' | 'fail' | ''
    onCancel: () => void
    onTest: () => void
    onSave: () => void
  } = $props()

  function openTokenPage(): void {
    const url = match(provider)
      .with('jira', () => 'https://id.atlassian.com/manage-profile/security/api-tokens')
      .with('youtrack', () =>
        baseUrl
          ? `${baseUrl.replace(/\/$/, '')}/hub/tokens`
          : 'https://youtrack.jetbrains.com/hub/tokens',
      )
      .with('github', () => 'https://github.com/settings/tokens')
      .exhaustive()
    window.api.openExternal(url)
  }
</script>

<div class="flex flex-col gap-2 p-3 border border-border rounded-md bg-bg-input">
  <div class="flex flex-col gap-1">
    <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
      >Provider</span
    >
    <CustomSelect
      value={provider}
      options={[
        { value: 'jira', label: 'Jira' },
        { value: 'youtrack', label: 'YouTrack' },
        { value: 'github', label: 'GitHub' },
      ]}
      onchange={(v) => (provider = v as Provider)}
    />
  </div>

  <div class="flex flex-col gap-1">
    <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
      >Base URL</span
    >
    <input
      class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
      name="baseUrl"
      aria-label="Base URL"
      bind:value={baseUrl}
      placeholder={provider === 'github' ? 'https://github.com' : 'https://company.atlassian.net'}
      spellcheck="false"
    />
  </div>

  {#if provider === 'github'}
    <div class="flex flex-col gap-1">
      <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
        >Repository (optional)</span
      >
      <input
        class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
        name="projectKey"
        aria-label="Repository"
        bind:value={projectKey}
        placeholder="owner/repo — auto-detected if empty"
        spellcheck="false"
      />
    </div>
  {/if}

  <div class="flex flex-col gap-2 pt-2 border-t border-border-subtle">
    <span class="text-2xs text-text-faint">Credentials — stored locally, never committed.</span>

    {#if provider === 'jira'}
      <div class="flex flex-col gap-1">
        <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
          >Email</span
        >
        <input
          class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
          name="username"
          aria-label="Email"
          bind:value={username}
          placeholder="user@company.com"
          spellcheck="false"
        />
      </div>
    {/if}

    <div class="flex flex-col gap-1">
      <div class="flex items-center justify-between gap-2">
        <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
          >API token</span
        >
        <button
          type="button"
          class="text-2xs text-accent-text bg-transparent border-0 p-0 cursor-pointer underline underline-offset-2 hover:text-accent"
          onclick={openTokenPage}
        >
          Generate →
        </button>
      </div>
      <input
        class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
        type="password"
        name="token"
        aria-label="API token"
        bind:value={token}
        placeholder={!isNew && hasExistingToken ? '••••••••' : 'Enter token'}
        autocomplete="off"
      />
    </div>
  </div>

  <div class="min-h-4.5" aria-live="polite">
    {#if testResult === 'success'}
      <span class="flex items-center gap-1 text-xs text-success"><Check size={13} /> OK</span>
    {:else if testResult === 'fail'}
      <span class="flex items-center gap-1 text-xs text-danger-text"><X size={13} /> Failed</span>
    {/if}
  </div>

  <div class="flex gap-1.5 justify-end">
    <button
      type="button"
      class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-transparent text-text-secondary hover:bg-hover hover:text-text"
      onclick={onCancel}>Cancel</button
    >
    <button
      type="button"
      class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-bg-input text-text-secondary enabled:hover:bg-hover-strong enabled:hover:text-text disabled:opacity-50 disabled:cursor-default"
      onclick={onTest}
      disabled={testing || !baseUrl || !token}
    >
      {testing ? 'Testing…' : 'Test'}
    </button>
    <button
      type="button"
      class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default"
      onclick={onSave}
      disabled={!baseUrl}>Save</button
    >
  </div>
</div>
