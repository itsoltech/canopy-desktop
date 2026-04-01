<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import BranchTokenBuilder from './BranchTokenBuilder.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'

  interface ConnectionInfo {
    id: string
    name: string
    provider: string
    baseUrl: string
    projectKey: string
    boardId?: string
    username?: string
  }

  let {
    connections,
    scopeBoards,
  }: {
    connections: ConnectionInfo[]
    scopeBoards: Record<string, Array<{ id: string; name: string }>>
  } = $props()

  type TemplateScope = 'global' | string
  let prScope = $state<TemplateScope>('global')

  function prPrefKey(scope: TemplateScope): string {
    return scope === 'global' ? 'taskTracker.pr' : `taskTracker.pr.${scope}`
  }

  interface PRConfig {
    titleTemplate: string
    bodyTemplate: string
    defaultBranch: string
  }

  const PR_TAGS = [
    { key: 'taskKey', description: 'Task key (e.g. ISSUE-123)', example: 'ISSUE-123' },
    { key: 'taskTitle', description: 'Task title', example: 'Fix login bug' },
    { key: 'taskType', description: 'Task type (task, bug, story)', example: 'task' },
    { key: 'boardKey', description: 'Board/project key', example: 'ISSUE' },
  ]

  let prConfig = $derived.by((): PRConfig => {
    const raw = prefs[prPrefKey(prScope)]
    if (raw) {
      try {
        const c = JSON.parse(raw) as Partial<PRConfig>
        return {
          titleTemplate: c.titleTemplate || '',
          bodyTemplate: c.bodyTemplate || '',
          defaultBranch: c.defaultBranch || 'develop',
        }
      } catch {
        // fall through
      }
    }
    return {
      titleTemplate: prefs['taskTracker.prTitleTemplate'] || '',
      bodyTemplate: prefs['taskTracker.prBodyTemplate'] || '',
      defaultBranch: prefs['taskTracker.prDefaultBranch'] || 'develop',
    }
  })

  let titleTemplateInput = $state('')
  let bodyTemplateInput = $state('')

  $effect(() => {
    titleTemplateInput = prConfig.titleTemplate
    bodyTemplateInput = prConfig.bodyTemplate
  })

  function savePRConfig(field: keyof PRConfig, value: string): void {
    const current = { ...prConfig, [field]: value }
    setPref(prPrefKey(prScope), JSON.stringify(current))
  }

  function onTitleTemplateSave(): void {
    savePRConfig('titleTemplate', titleTemplateInput)
  }

  function onBodyTemplateSave(): void {
    savePRConfig('bodyTemplate', bodyTemplateInput)
  }
</script>

<div class="section">
  <h3 class="section-title">Pull Request Naming</h3>
  <p class="section-desc">Configure per board, connection, or globally.</p>

  <div class="form-row">
    <label class="form-label">Scope</label>
    <CustomSelect
      value={prScope}
      groups={[
        { label: '', options: [{ value: 'global', label: 'Global (default)' }] },
        ...connections.map((conn) => ({
          label: conn.name,
          options: [
            { value: conn.id, label: 'All boards' },
            ...(scopeBoards[conn.id] ?? []).map((b) => ({
              value: `${conn.id}.${b.id}`,
              label: b.name,
            })),
          ],
        })),
      ]}
      onchange={(v) => (prScope = v)}
      maxWidth="none"
    />
  </div>

  <BranchTokenBuilder
    bind:templateInput={titleTemplateInput}
    placeholders={PR_TAGS}
    onSave={onTitleTemplateSave}
    label="Title"
    autoSeparators={false}
  />

  <BranchTokenBuilder
    bind:templateInput={bodyTemplateInput}
    placeholders={PR_TAGS}
    onSave={onBodyTemplateSave}
    label="Body"
    autoSeparators={false}
  />
  <div class="body-editor">
    <label class="form-label">Body edit</label>
    <textarea
      class="form-textarea"
      bind:value={bodyTemplateInput}
      oninput={onBodyTemplateSave}
      rows="6"
      placeholder="PR body template — use tags above or type freely"
    ></textarea>
  </div>

  <div class="form-row">
    <label class="form-label">Default Target Branch</label>
    <input
      class="form-input"
      value={prConfig.defaultBranch}
      oninput={(e) => savePRConfig('defaultBranch', (e.target as HTMLInputElement).value)}
      placeholder="develop"
    />
  </div>
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

  .body-editor {
    display: flex;
    gap: 8px;
    margin-bottom: 8px;
  }

  .form-textarea {
    flex: 1;
    padding: 6px 8px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-bg-input);
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
    outline: none;
    resize: vertical;
    min-height: 100px;
  }

  .form-textarea:focus {
    border-color: var(--c-focus-ring);
  }

  .form-textarea::placeholder {
    color: var(--c-text-faint);
  }
</style>
