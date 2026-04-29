<script lang="ts">
  import AgentProfilesPanel from './AgentProfilesPanel.svelte'
  import CodexSdkProfileForm from './CodexSdkProfileForm.svelte'
</script>

<p class="section-hint">
  The SDK-backed Codex pane runs OpenAI's Codex SDK in the Canopy main process. It is separate from
  the terminal-backed Codex CLI tool.
</p>

<AgentProfilesPanel agentType="codex-sdk" title="Codex Agent" form={CodexSdkProfileForm} />
