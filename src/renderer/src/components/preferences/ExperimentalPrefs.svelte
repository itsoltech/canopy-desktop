<script lang="ts">
  import { FlaskConical } from '@lucide/svelte'
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'

  let sdkAgentsEnabled = $derived(prefs['experimental.sdkAgents'] === 'true')

  function toggleSdkAgents(): void {
    setPref('experimental.sdkAgents', sdkAgentsEnabled ? 'false' : 'true')
  }
</script>

<div class="section">
  <h3 class="section-title">
    <FlaskConical size={16} />
    <span>Experimental</span>
  </h3>

  <p class="warning">
    Features in this section are in active development. Expect rough edges, incomplete UI, and
    breaking changes between releases. Turn them off if something looks wrong.
  </p>

  <label class="checkbox-row">
    <CustomCheckbox checked={sdkAgentsEnabled} onchange={toggleSdkAgents} />
    <span>In-process SDK agents (Claude and Codex)</span>
  </label>
  {#if sdkAgentsEnabled}
    <div class="hint-row">
      Enables SDK chat panes that run agent SDKs in the Canopy main process. Conversations persist
      to the local SQLite database and can be searched via FTS5. Uses profiles tagged 'claude-sdk'
      and 'codex-sdk'.
    </div>
  {:else}
    <div class="hint-row">
      When enabled, unlocks the SDK chat pane and surfaces its entry in the new-pane picker.
    </div>
  {/if}
</div>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .section-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 15px;
    font-weight: 600;
    color: var(--c-text);
    margin: 0;
  }

  .warning {
    margin: 0;
    padding: 8px 12px;
    border-left: 3px solid var(--c-warning);
    background: color-mix(in srgb, var(--c-warning) 10%, transparent);
    border-radius: 4px;
    color: var(--c-text);
    font-size: 12px;
    line-height: 1.5;
  }

  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--c-text);
    cursor: pointer;
  }

  .hint-row {
    font-size: 11px;
    color: var(--c-text-muted);
    line-height: 1.5;
    padding-left: 24px;
    margin-top: -8px;
  }
</style>
