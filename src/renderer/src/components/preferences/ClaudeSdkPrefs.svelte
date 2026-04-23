<script lang="ts">
  import AgentProfilesPanel from './AgentProfilesPanel.svelte'
  import ClaudeProfileForm from './ClaudeProfileForm.svelte'
</script>

<!--
  The SDK-backed Claude pane reuses the same profile shape as the CLI-backed
  Claude profile. API key is optional: when empty, the bundled Claude CLI
  binary that ships with @anthropic-ai/claude-agent-sdk handles auth from its
  own stored session (same as running `claude` in a terminal). Set an API key
  only when you need to override that with a specific account or use a
  non-default baseUrl / provider. The SDK-only `mcpServers` field isn't in
  the form yet — configure via DB until the form grows a dedicated input.
-->
<AgentProfilesPanel agentType="claude-sdk" title="Claude Agent" form={ClaudeProfileForm} />
