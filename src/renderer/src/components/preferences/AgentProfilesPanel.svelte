<script lang="ts">
  import { tick, type Component } from 'svelte'
  import type { AgentType } from '../../../../main/agents/types'
  import type { AgentProfileMasked, ProfilePrefs } from '../../../../main/profiles/types'
  import { getProfilesByAgent, saveProfile, deleteProfile } from '../../lib/stores/profiles.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'

  type FormProps = {
    prefs: ProfilePrefs
    apiKey: string
    hasApiKey: boolean
    onPrefsChange: (next: ProfilePrefs) => void
    onApiKeyChange: (next: string) => void
  }

  let {
    agentType,
    title,
    form: FormComponent,
  }: {
    agentType: AgentType
    title: string
    form: Component<FormProps>
  } = $props()

  let agentProfiles = $derived(getProfilesByAgent(agentType))
  let selectedId: string | null = $state(null)
  let nameInputEl: HTMLInputElement | undefined = $state()

  $effect(() => {
    if (agentProfiles.length === 0) {
      selectedId = null
      return
    }
    if (!selectedId || !agentProfiles.some((p) => p.id === selectedId)) {
      selectedId = agentProfiles[0].id
    }
  })

  let selected = $derived(agentProfiles.find((p) => p.id === selectedId) ?? null)

  let draftName = $state('')
  let draftPrefs: ProfilePrefs = $state({})
  let draftApiKey = $state('')
  let draftApiKeyTouched = $state(false)
  let saving = $state(false)
  let dirty = $state(false)

  $effect(() => {
    if (!selected) {
      draftName = ''
      draftPrefs = {}
      draftApiKey = ''
      draftApiKeyTouched = false
      dirty = false
      return
    }
    draftName = selected.name
    draftPrefs = { ...selected.prefs }
    draftApiKey = ''
    draftApiKeyTouched = false
    dirty = false
  })

  function markDirty(): void {
    dirty = true
  }

  function onPrefsChange(next: ProfilePrefs): void {
    draftPrefs = next
    markDirty()
  }

  function onApiKeyChange(next: string): void {
    draftApiKey = next
    draftApiKeyTouched = true
    markDirty()
  }

  function onNameInput(e: Event): void {
    draftName = (e.target as HTMLInputElement).value
    markDirty()
  }

  async function handleSave(): Promise<void> {
    if (!selected) return
    if (!draftName.trim()) {
      addToast('Profile name is required')
      return
    }
    saving = true
    try {
      const apiKey = draftApiKeyTouched ? (draftApiKey === '' ? null : draftApiKey) : undefined
      const saved = await saveProfile({
        id: selected.id,
        agentType,
        name: draftName.trim(),
        prefs: $state.snapshot(draftPrefs) as ProfilePrefs,
        apiKey,
      })
      selectedId = saved.id
      dirty = false
      draftApiKeyTouched = false
      addToast(`Saved profile "${saved.name}"`)
    } catch (e) {
      addToast(`Save failed: ${e instanceof Error ? e.message : String(e)}`)
    } finally {
      saving = false
    }
  }

  function uniqueName(base: string): string {
    const existing = new Set(agentProfiles.map((p) => p.name))
    if (!existing.has(base)) return base
    for (let i = 2; i < 1000; i++) {
      const candidate = `${base} ${i}`
      if (!existing.has(candidate)) return candidate
    }
    return `${base} ${Date.now()}`
  }

  async function handleNew(): Promise<void> {
    saving = true
    try {
      const created = await saveProfile({
        agentType,
        name: uniqueName('New profile'),
        prefs: {},
      })
      selectedId = created.id
      await tick()
      nameInputEl?.focus()
      nameInputEl?.select()
    } catch (e) {
      addToast(`Create failed: ${e instanceof Error ? e.message : String(e)}`)
    } finally {
      saving = false
    }
  }

  async function selectProfile(id: string): Promise<void> {
    if (id === selectedId) return
    if (dirty) {
      const ok = await confirm({
        title: 'Discard unsaved changes?',
        message: 'You have unsaved changes in the current profile. Switch anyway?',
        confirmLabel: 'Discard',
        destructive: true,
      })
      if (!ok) return
    }
    selectedId = id
  }

  async function handleDelete(profile: AgentProfileMasked): Promise<void> {
    const ok = await confirm({
      title: 'Delete profile',
      message: `Delete profile "${profile.name}"? This cannot be undone.`,
      confirmLabel: 'Delete',
      destructive: true,
    })
    if (!ok) return
    try {
      await deleteProfile(profile.id)
      if (selectedId === profile.id) selectedId = null
    } catch (e) {
      addToast(`Delete failed: ${e instanceof Error ? e.message : String(e)}`)
    }
  }
</script>

<div class="flex flex-col gap-4 h-full">
  <h3 class="text-[15px] font-semibold text-text m-0">{title}</h3>
  <p class="text-sm text-text-faint m-0 leading-snug">
    Create multiple profiles to switch between providers (Anthropic, Ollama, GLM, etc.) without
    re-entering settings. Click a profile in the sidebar to launch it.
  </p>

  <div class="grid grid-cols-[200px_1fr] gap-4 flex-1 min-h-0">
    <aside class="flex flex-col gap-2 border-r border-border pr-3 min-h-0">
      <div class="flex items-center justify-between gap-2">
        <span class="text-xs font-semibold uppercase tracking-[0.5px] text-text-muted"
          >Profiles</span
        >
        <button
          class="px-2.5 py-1 border border-dashed border-text-faint rounded-lg bg-transparent text-text-secondary text-sm font-inherit cursor-pointer enabled:hover:bg-hover enabled:hover:text-text disabled:opacity-50 disabled:cursor-not-allowed"
          onclick={handleNew}
          disabled={saving}>+ New</button
        >
      </div>
      {#if agentProfiles.length === 0}
        <div class="text-sm text-text-faint p-2">No profiles yet</div>
      {:else}
        <ul class="list-none m-0 p-0 flex flex-col gap-0.5 overflow-y-auto group/profilelist">
          {#each agentProfiles as p (p.id)}
            <li
              class="flex items-center rounded-lg bg-transparent group/profilerow hover:bg-hover"
              class:!bg-active={selectedId === p.id}
            >
              <button
                class="flex-1 flex items-center gap-1.5 px-2 py-1.5 border-0 bg-transparent text-text font-inherit text-md text-left cursor-pointer min-w-0"
                onclick={() => selectProfile(p.id)}
                title={p.name}
              >
                <span class="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{p.name}</span>
                {#if p.isDefault}
                  <span
                    class="text-2xs uppercase text-text-faint border border-border rounded-sm px-1"
                    >default</span
                  >
                {/if}
              </button>
              <button
                class="invisible group-hover/profilerow:visible border-0 bg-transparent text-text-faint text-sm cursor-pointer px-2 py-1.5 mr-1 rounded-sm flex-shrink-0 hover:text-danger-text hover:bg-danger-bg"
                title="Delete"
                onclick={() => handleDelete(p)}
                aria-label="Delete profile {p.name}"
              >
                ✕
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </aside>

    <section class="flex flex-col gap-4 overflow-y-auto min-h-0">
      {#if selected}
        <div class="flex items-end gap-3 pb-3 border-b border-border">
          <div class="flex flex-col gap-1 flex-1">
            <label class="text-sm font-medium text-text-secondary" for="profile-name"
              >Profile name</label
            >
            <input
              id="profile-name"
              class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
              type="text"
              bind:this={nameInputEl}
              value={draftName}
              oninput={onNameInput}
              spellcheck="false"
            />
          </div>
          <button
            class="px-4 py-1.5 border-0 rounded-lg bg-accent-bg text-accent-text text-md font-inherit cursor-pointer enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-not-allowed"
            onclick={handleSave}
            disabled={saving || !dirty}
          >
            {saving ? 'Saving…' : 'Save'}
          </button>
        </div>

        <FormComponent
          prefs={draftPrefs}
          apiKey={draftApiKey}
          hasApiKey={selected.hasApiKey}
          {onPrefsChange}
          {onApiKeyChange}
        />
      {:else}
        <div class="text-md text-text-faint p-6 text-center">
          Select or create a profile to begin.
        </div>
      {/if}
    </section>
  </div>
</div>
