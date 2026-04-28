<script lang="ts">
  import { onMount } from 'svelte'
  import { Eye, EyeOff, Trash2 } from '@lucide/svelte'
  import { confirm } from '../../../lib/stores/dialogs.svelte'
  import PrefsSection from './PrefsSection.svelte'
  import { prefsSearch, matches } from './prefsSearch.svelte'

  interface Credential {
    id: string
    domain: string
    username: string
    title: string
    createdAt: string
    updatedAt: string
  }

  let credentials: Credential[] = $state([])
  let revealedId: string | null = $state(null)
  let revealedPassword = $state('')
  let revealTimer: ReturnType<typeof setTimeout> | null = null

  async function loadCredentials(): Promise<void> {
    credentials = await window.api.listCredentials()
  }

  async function deleteCredential(id: string, domain: string, username: string): Promise<void> {
    const ok = await confirm({
      title: 'Delete saved password',
      message: `Delete the saved password for ${username} on ${domain}? This cannot be undone.`,
      confirmLabel: 'Delete',
      destructive: true,
    })
    if (!ok) return
    await window.api.deleteCredential(id)
    await loadCredentials()
  }

  async function revealPassword(id: string, domain: string): Promise<void> {
    if (revealedId === id) {
      revealedId = null
      revealedPassword = ''
      if (revealTimer) clearTimeout(revealTimer)
      return
    }
    const cred = await window.api.getCredentialDecrypted(id, domain, 'reveal')
    if (cred) {
      revealedId = id
      revealedPassword = cred.password
      if (revealTimer) clearTimeout(revealTimer)
      revealTimer = setTimeout(() => {
        revealedId = null
        revealedPassword = ''
      }, 5000)
    }
  }

  onMount(() => {
    loadCredentials()
    return () => {
      if (revealTimer) clearTimeout(revealTimer)
    }
  })

  function visible(cred: Credential): boolean {
    if (prefsSearch.query.trim() === '') return true
    return matches(`${cred.domain} ${cred.username} ${cred.title}`)
  }
</script>

<PrefsSection
  title="Saved passwords"
  description="Browser-pane credentials stored encrypted via Electron safeStorage"
>
  <div class="flex flex-col gap-1">
    {#each credentials as cred (cred.id)}
      <div
        class="flex flex-col gap-1 px-3 py-2 rounded-md bg-bg-input border border-border-subtle transition-opacity duration-fast"
        class:opacity-30={!visible(cred)}
      >
        <div class="flex items-baseline gap-2 min-w-0">
          <span class="text-md text-text truncate">{cred.domain}</span>
          {#if cred.title}
            <span class="text-xs text-text-faint truncate">{cred.title}</span>
          {/if}
        </div>
        <div class="flex items-center gap-2">
          <span class="text-sm text-text-secondary truncate flex-1">{cred.username}</span>
          <span class="shrink-0">
            {#if revealedId === cred.id}
              <code class="text-xs text-text bg-bg px-1 py-px rounded-sm font-mono"
                >{revealedPassword}</code
              >
            {:else}
              <span class="text-text-faint text-sm tracking-wider">••••••••</span>
            {/if}
          </span>
          <button
            type="button"
            class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text"
            onclick={() => revealPassword(cred.id, cred.domain)}
            title={revealedId === cred.id ? 'Hide password' : 'Show password (5s)'}
            aria-label={revealedId === cred.id ? 'Hide password' : 'Show password'}
          >
            {#if revealedId === cred.id}
              <EyeOff size={13} />
            {:else}
              <Eye size={13} />
            {/if}
          </button>
          <button
            type="button"
            class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-danger-bg hover:text-danger-text"
            onclick={() => deleteCredential(cred.id, cred.domain, cred.username)}
            aria-label="Remove credential for {cred.username}"
            title="Remove"
          >
            <Trash2 size={13} />
          </button>
        </div>
      </div>
    {:else}
      <p class="text-sm text-text-faint m-0 py-2">No saved passwords</p>
    {/each}
  </div>
</PrefsSection>
