<script lang="ts">
  import { Check, X } from '@lucide/svelte'
  import TrackerProviderIcon from '../../shared/TrackerProviderIcon.svelte'

  // Add/edit form for a personal CI server connection — the CI counterpart of
  // TrackerEditForm. Single provider today; the Provider row becomes a select once
  // a second CI provider exists.

  let {
    url = $bindable(),
    token = $bindable(),
    isNew,
    urlValid,
    testing,
    testResult,
    onCancel,
    onTest,
    saving = false,
    onSave,
    onOpenTokenPage,
  }: {
    url: string
    token: string
    isNew: boolean
    urlValid: boolean
    testing: boolean
    testResult: 'success' | 'fail' | ''
    onCancel: () => void
    onTest: () => void
    /** Save in flight — the button says so, like Test's "Testing…". */
    saving?: boolean
    onSave: () => void
    onOpenTokenPage: () => void
  } = $props()

  const URL_REQUIRED = 'Disabled: enter a valid server URL first'
  const TOKEN_REQUIRED = 'Disabled: enter an access token first'

  // Both actions share these preconditions. Keep the reason visible and associated
  // with each aria-disabled button because title text is unavailable on keyboard focus.
  let tokenPresent = $derived(token.trim().length > 0)
  let formBlockedReason = $derived(!urlValid ? URL_REQUIRED : !tokenPresent ? TOKEN_REQUIRED : '')
</script>

<div class="flex flex-col gap-2 p-3 border border-border rounded-md bg-bg-input">
  <div class="flex flex-col gap-1">
    <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
      >Provider</span
    >
    <!-- Static until a second CI provider exists — a single-option combobox is a
         dead Tab stop with a no-op handler. -->
    <span class="inline-flex items-center gap-1.5 px-2.5 py-1.5 text-sm text-text-secondary">
      <TrackerProviderIcon provider="teamcity" size={14} />
      TeamCity
    </span>
  </div>

  <div class="flex flex-col gap-1">
    <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
      >Server URL</span
    >
    {#if isNew}
      <input
        class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
        name="ciConnectionUrl"
        aria-label="CI server URL"
        bind:value={url}
        placeholder="https://teamcity.example.com"
        spellcheck="false"
      />
    {:else}
      <span class="px-2.5 py-1.5 text-sm text-text-secondary truncate" title={url}>{url}</span>
    {/if}
  </div>

  <div class="flex flex-col gap-2 pt-2 border-t border-border-subtle">
    <div class="flex flex-col gap-1">
      <div class="flex items-center justify-between gap-2">
        <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
          >Access token</span
        >
        <button
          type="button"
          class="text-2xs text-accent-text bg-transparent border-0 p-0 cursor-pointer underline underline-offset-2 hover:text-accent disabled:opacity-50 disabled:cursor-default disabled:no-underline"
          onclick={onOpenTokenPage}
          disabled={!urlValid}
          title={urlValid ? 'Open the token page on this server' : 'Enter the server URL first'}
        >
          Generate →
        </button>
      </div>
      <input
        class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
        type="password"
        name="ciConnectionToken"
        aria-label="Access token"
        bind:value={token}
        placeholder={isNew ? 'Enter token' : '••••••••'}
        autocomplete="off"
        title="Stored encrypted on your machine, keyed by provider + URL — never written to your repository"
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

  <div class="min-h-4">
    {#if formBlockedReason}
      <span id="ci-server-form-blocked" class="text-xs text-text-secondary break-words"
        >{formBlockedReason}</span
      >
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
      class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-bg text-text-secondary hover:bg-hover-strong hover:text-text aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-bg aria-disabled:hover:text-text-secondary"
      onclick={onTest}
      aria-disabled={testing || !urlValid || !tokenPresent}
      aria-busy={testing}
      aria-describedby={formBlockedReason ? 'ci-server-form-blocked' : undefined}
      title={testing
        ? 'Testing the connection…'
        : !urlValid
          ? URL_REQUIRED
          : !tokenPresent
            ? TOKEN_REQUIRED
            : 'Check the connection against the server — nothing is saved'}
    >
      {testing ? 'Testing…' : 'Test'}
    </button>
    <button
      type="button"
      class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text hover:bg-accent-bg-hover aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-accent-bg"
      onclick={onSave}
      aria-disabled={saving || !urlValid || !tokenPresent}
      aria-busy={saving}
      aria-describedby={formBlockedReason ? 'ci-server-form-blocked' : undefined}
      title={saving
        ? 'Saving…'
        : !urlValid
          ? URL_REQUIRED
          : !tokenPresent
            ? TOKEN_REQUIRED
            : 'Save the token (stored globally on this machine, per provider + URL)'}
      >{saving ? 'Saving…' : isNew ? 'Add connection' : 'Save token'}</button
    >
  </div>
</div>
