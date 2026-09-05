<script lang="ts">
  import { Check, LoaderCircle, Trash2, TriangleAlert } from '@lucide/svelte'
  import TrackerProviderIcon from '../../shared/TrackerProviderIcon.svelte'

  interface Server {
    provider: string
    baseUrl: string
    capabilities: string[]
    verification: Record<string, { state: string; checkedAt: string; reason?: string }>
    bindings: string[]
  }

  interface Props {
    server: Server
    credentialIssue: string
    removingUrl: string
    onEdit: () => void
    onRemove: () => void
    class?: string
  }

  let {
    server,
    credentialIssue,
    removingUrl,
    onEdit,
    onRemove,
    class: className = '',
  }: Props = $props()

  let removalKey = $derived(`${server.provider}:${server.baseUrl}`)
</script>

<div class={`flex flex-col gap-0.5 ${className}`}>
  <div class="flex items-center gap-1">
    <button
      type="button"
      class="flex-1 flex items-center gap-2 px-2.5 py-1.5 border border-border-subtle rounded-md bg-bg-input text-text text-sm font-inherit cursor-pointer text-left hover:border-border aria-disabled:cursor-default aria-disabled:hover:border-border-subtle min-w-0"
      onclick={onEdit}
      aria-disabled={server.provider !== 'teamcity'}
      title={server.provider === 'teamcity'
        ? 'Update the stored token for this server'
        : 'GitHub token — update it from a repository GitHub Actions configurator'}
    >
      <span
        class="inline-flex items-center shrink-0 text-text-muted"
        title={server.provider === 'github-actions' ? 'GitHub Actions' : 'TeamCity'}
      >
        <TrackerProviderIcon provider={server.provider} size={14} />
      </span>
      <span class="flex-1 text-text-secondary truncate" title={server.baseUrl}
        >{server.baseUrl}</span
      >
      <span
        class="text-2xs text-text-faint shrink-0"
        title={`Capabilities: ${server.capabilities.map((capability) => `${capability} (${server.verification[capability]?.state ?? 'unverified'})`).join(', ')}. Bindings: ${server.bindings.join(', ') || 'none'}`}
        >{server.provider === 'github-actions'
          ? 'Actions · repo scoped'
          : 'Builds · server scoped'}</span
      >
      {#if credentialIssue}
        <span class="flex items-center gap-1 text-2xs text-warning-text shrink-0">
          <TriangleAlert size={12} /> Needs attention
        </span>
      {:else}
        <span
          class="flex items-center gap-1 text-2xs text-success shrink-0"
          title="Credentials saved"
        >
          <Check size={12} />
        </span>
      {/if}
    </button>
    <!-- aria-disabled: a real disabled makes ConfirmDialog's focus restore a no-op
         (.focus() on a disabled element does nothing), stranding the user on <body>
         after confirming. The parent's guard blocks re-entry. -->
    <button
      type="button"
      class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-danger-bg hover:text-danger-text aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-transparent aria-disabled:hover:text-text-muted"
      onclick={onRemove}
      aria-disabled={removingUrl !== ''}
      aria-busy={removingUrl === removalKey}
      aria-label="Remove CI connection"
      title={removingUrl !== ''
        ? removingUrl === removalKey
          ? 'Removing…'
          : 'Disabled while another connection is being removed'
        : 'Remove the stored token for this server'}
    >
      {#if removingUrl === removalKey}
        <LoaderCircle size={12} class="animate-spin-slow motion-reduce:animate-none" />
      {:else}
        <Trash2 size={12} />
      {/if}
    </button>
  </div>
  {#if credentialIssue}
    <p class="m-0 px-2.5 text-2xs text-warning-text break-words">
      {credentialIssue}
    </p>
  {/if}
</div>
