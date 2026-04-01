<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'

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
    defaultBranch: string
  }

  let prConfig = $derived.by((): PRConfig => {
    const raw = prefs[prPrefKey(prScope)]
    if (raw) {
      try {
        const c = JSON.parse(raw) as Partial<PRConfig>
        return { defaultBranch: c.defaultBranch || 'develop' }
      } catch {
        // fall through
      }
    }
    return { defaultBranch: prefs['taskTracker.prDefaultBranch'] || 'develop' }
  })

  function savePRConfig(field: keyof PRConfig, value: string): void {
    const current = { ...prConfig, [field]: value }
    setPref(prPrefKey(prScope), JSON.stringify(current))
  }
</script>

<div class="section">
  <h3 class="section-title">Pull Request Naming</h3>
  <p class="section-desc">Configure per board, connection, or globally.</p>

  <div class="form-row">
    <label class="form-label">Scope</label>
    <select class="form-select" bind:value={prScope}>
      <option value="global">Global (default)</option>
      {#each connections as conn (conn.id)}
        <optgroup label={conn.name}>
          <option value={conn.id}>All boards</option>
          {#each scopeBoards[conn.id] ?? [] as board (board.id)}
            <option value="{conn.id}.{board.id}">{board.name}</option>
          {/each}
        </optgroup>
      {/each}
    </select>
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

  .form-input,
  .form-select {
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

  .form-input:focus,
  .form-select:focus {
    border-color: var(--c-focus-ring);
  }

  .form-select {
    cursor: pointer;
  }
</style>
