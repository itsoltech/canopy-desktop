<script lang="ts">
  import { tick, type Component } from 'svelte'
  import { Plus, Trash2 } from '@lucide/svelte'
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
    form: FormComponent,
  }: {
    agentType: AgentType
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

<div class="grid grid-cols-[180px_1fr] gap-5 h-full min-h-0">
  <aside class="flex flex-col gap-2 border-r border-border-subtle pr-3 min-h-0">
    <div class="flex items-center justify-between gap-2 pl-1">
      <span class="text-2xs font-semibold uppercase tracking-caps-looser text-text-faint">
        Profiles
      </span>
      <button
        type="button"
        class="flex items-center justify-center size-6 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text disabled:opacity-50 disabled:cursor-default"
        onclick={handleNew}
        disabled={saving}
        aria-label="New profile"
        title="New profile"
      >
        <Plus size={14} />
      </button>
    </div>

    {#if agentProfiles.length === 0}
      <div class="text-sm text-text-faint px-2 py-3">No profiles yet</div>
    {:else}
      <ul role="list" class="m-0 p-0 flex flex-col gap-1 overflow-y-auto">
        {#each agentProfiles as p (p.id)}
          {@const active = selectedId === p.id}
          <li
            class="group/profilerow flex items-center rounded-md hover:bg-hover"
            class:bg-accent-bg={active}
          >
            <button
              type="button"
              class="flex-1 flex items-center gap-1.5 px-2 py-1 border-0 bg-transparent font-inherit text-md text-left cursor-pointer min-w-0"
              class:text-text={!active}
              class:text-accent-text={active}
              onclick={() => selectProfile(p.id)}
              title={p.name}
            >
              <span class="flex-1 truncate">{p.name}</span>
              {#if p.isDefault}
                <span
                  class="text-2xs uppercase tracking-caps-tight text-text-faint border border-border-subtle rounded-sm px-1 shrink-0"
                  >default</span
                >
              {/if}
            </button>
            <button
              type="button"
              class="invisible group-hover/profilerow:visible flex items-center justify-center size-6 mr-1 border-0 bg-transparent rounded-sm text-text-faint cursor-pointer shrink-0 hover:text-danger-text hover:bg-danger-bg"
              title="Delete"
              onclick={() => handleDelete(p)}
              aria-label="Delete profile {p.name}"
            >
              <Trash2 size={12} />
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </aside>

  <section class="flex flex-col min-h-0">
    {#if selected}
      <div class="flex items-end gap-3 pb-4 mb-7 border-b border-border-subtle shrink-0">
        <div class="flex flex-col gap-1 flex-1 min-w-0">
          <label
            class="text-2xs font-semibold uppercase tracking-caps-looser text-text-faint"
            for="profile-name">Profile name</label
          >
          <input
            id="profile-name"
            class="px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-md font-inherit outline-none focus:border-focus-ring"
            type="text"
            name="profileName"
            bind:this={nameInputEl}
            value={draftName}
            oninput={onNameInput}
            spellcheck="false"
          />
        </div>
        <div class="flex items-center gap-2 shrink-0">
          {#if dirty}
            <span class="text-xs text-warning-text">Unsaved changes</span>
          {/if}
          <button
            type="button"
            class="px-3.5 py-1.5 rounded-md text-md font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default"
            onclick={handleSave}
            disabled={saving || !dirty}
          >
            {saving ? 'Saving…' : 'Save'}
          </button>
        </div>
      </div>

      <div class="flex-1 overflow-y-auto pr-1 py-2">
        <FormComponent
          prefs={draftPrefs}
          apiKey={draftApiKey}
          hasApiKey={selected.hasApiKey}
          {onPrefsChange}
          {onApiKeyChange}
        />
      </div>
    {:else}
      <div class="text-md text-text-faint p-6 text-center">
        Select or create a profile to begin.
      </div>
    {/if}
  </section>
</div>
