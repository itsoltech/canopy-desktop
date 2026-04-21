<script lang="ts">
  import AgentProfilesPanel from './AgentProfilesPanel.svelte'
  import ClaudeProfileForm from './ClaudeProfileForm.svelte'
</script>

<!--
  The SDK-backed Claude pane reuses the same profile shape as the CLI-backed
  Claude profile (apiKey + permissionMode + model + baseUrl + provider +
  customEnv). The only SDK-only field — `mcpServers` — isn't in the form yet;
  configure it directly via the DB until the form grows a dedicated input.
-->
<AgentProfilesPanel agentType="claude-sdk" title="Claude (SDK)" form={ClaudeProfileForm} />
