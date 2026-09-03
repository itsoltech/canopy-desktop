<script lang="ts">
  import CustomSelect from '../../shared/CustomSelect.svelte'
  import type { ProjectCiModalState } from '../projectCiModalState.svelte'

  let { state, class: className = '' }: { state: ProjectCiModalState; class?: string } = $props()
</script>

<section class={`rounded-lg border border-border-subtle p-4 flex flex-col gap-3 ${className}`}>
  <div>
    <h3 class="m-0 text-sm font-semibold text-text">Shared TeamCity server</h3>
    <p class="m-0 mt-0.5 text-xs text-text-muted leading-snug">
      The selected server is stored for everyone in this repository's
      <code class="font-mono">.canopy/config.json</code>.
    </p>
  </div>
  <div class="flex flex-col gap-1">
    <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint">
      Server
    </span>
    <CustomSelect
      value={state.selectedServer}
      options={state.serverOptions}
      onchange={state.selectServer}
    />
  </div>

  {#if state.selectedServer === state.newServerValue}
    <div class="flex flex-col gap-1">
      <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint">
        Server URL
      </span>
      <input
        class="px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
        name="ciModalUrl"
        aria-label="TeamCity server URL"
        bind:value={state.newUrl}
        placeholder="https://teamcity.example.com"
        spellcheck="false"
      />
    </div>
  {/if}
</section>
