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

  // Pick a sensible default selection when the list updates
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

  // Draft state: mirrors selected profile, mutated by the form
  let draftName = $state('')
  let draftPrefs: ProfilePrefs = $state({})
  let draftApiKey = $state('')
  let draftApiKeyTouched = $state(false)
  let saving = $state(false)
  let dirty = $state(false)

  // Sync draft when selection changes
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

<div class="panel">
  <h3 class="section-title">{title}</h3>
  <p class="section-hint">
    Create multiple profiles to switch between providers (Anthropic, Ollama, GLM, etc.) without
    re-entering settings. Click a profile in the sidebar to launch it.
  </p>

  <div class="layout">
    <aside class="list-pane">
      <div class="list-header">
        <span class="list-title">Profiles</span>
        <button class="btn-new" onclick={handleNew} disabled={saving}>+ New</button>
      </div>
      {#if agentProfiles.length === 0}
        <div class="empty">No profiles yet</div>
      {:else}
        <ul class="profile-list">
          {#each agentProfiles as p (p.id)}
            <li class="profile-row" class:selected={selectedId === p.id}>
              <button class="profile-row-select" onclick={() => selectProfile(p.id)} title={p.name}>
                <span class="profile-name">{p.name}</span>
                {#if p.isDefault}
                  <span class="badge-default">default</span>
                {/if}
              </button>
              <button
                class="row-delete"
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

    <section class="form-pane">
      {#if selected}
        <div class="form-header">
          <div class="field">
            <label class="field-label" for="profile-name">Profile name</label>
            <input
              id="profile-name"
              class="text-input"
              type="text"
              bind:this={nameInputEl}
              value={draftName}
              oninput={onNameInput}
              spellcheck="false"
            />
          </div>
          <button class="btn-save" onclick={handleSave} disabled={saving || !dirty}>
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
        <div class="empty-form">Select or create a profile to begin.</div>
      {/if}
    </section>
  </div>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 16px;
    height: 100%;
  }

  .section-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text);
    margin: 0;
  }

  .section-hint {
    font-size: 12px;
    color: var(--color-text-faint);
    margin: 0;
    line-height: 1.4;
  }

  .layout {
    display: grid;
    grid-template-columns: 200px 1fr;
    gap: 16px;
    flex: 1;
    min-height: 0;
  }

  .list-pane {
    display: flex;
    flex-direction: column;
    gap: 8px;
    border-right: 1px solid var(--color-border);
    padding-right: 12px;
    min-height: 0;
  }

  .list-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .list-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--color-text-muted);
  }

  .btn-new {
    padding: 4px 10px;
    border: 1px dashed var(--color-text-faint);
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-secondary);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
  }

  .btn-new:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .btn-new:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .profile-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow-y: auto;
  }

  .profile-row {
    display: flex;
    align-items: center;
    border-radius: 6px;
    background: transparent;
  }

  .profile-row:hover {
    background: var(--color-hover);
  }

  .profile-row.selected {
    background: var(--color-active);
  }

  .profile-row-select {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    border: none;
    background: transparent;
    color: var(--color-text);
    font-family: inherit;
    font-size: 13px;
    text-align: left;
    cursor: pointer;
    min-width: 0;
  }

  .profile-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .badge-default {
    font-size: 10px;
    text-transform: uppercase;
    color: var(--color-text-faint);
    border: 1px solid var(--color-border);
    border-radius: 3px;
    padding: 0 4px;
  }

  .row-delete {
    border: none;
    background: transparent;
    color: var(--color-text-faint);
    font-size: 12px;
    cursor: pointer;
    padding: 6px 8px;
    margin-right: 4px;
    border-radius: 3px;
    visibility: hidden;
    flex-shrink: 0;
  }

  .profile-row:hover .row-delete {
    visibility: visible;
  }

  .row-delete:hover {
    color: var(--color-danger-text);
    background: var(--color-danger-bg);
  }

  .empty {
    font-size: 12px;
    color: var(--color-text-faint);
    padding: 8px;
  }

  .form-pane {
    display: flex;
    flex-direction: column;
    gap: 16px;
    overflow-y: auto;
    min-height: 0;
  }

  .form-header {
    display: flex;
    align-items: flex-end;
    gap: 12px;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--color-border);
  }

  .form-header .field {
    flex: 1;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .field-label {
    font-size: 12px;
    font-weight: 500;
    color: var(--color-text-secondary);
  }

  .text-input {
    padding: 6px 10px;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: var(--color-hover);
    color: var(--color-text);
    font-size: 13px;
    font-family: inherit;
    outline: none;
  }

  .text-input:focus {
    border-color: var(--color-focus-ring);
  }

  .btn-save {
    padding: 6px 16px;
    border: none;
    border-radius: 6px;
    background: var(--color-accent-bg);
    color: var(--color-accent-text);
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
  }

  .btn-save:hover:not(:disabled) {
    background: var(--color-accent-bg-hover);
  }

  .btn-save:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .empty-form {
    font-size: 13px;
    color: var(--color-text-faint);
    padding: 24px;
    text-align: center;
  }
</style>
