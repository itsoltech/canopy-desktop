<script lang="ts">
  import AgentProfilesPanel from './AgentProfilesPanel.svelte'
  import ClaudeProfileForm from './ClaudeProfileForm.svelte'
</script>

<AgentProfilesPanel agentType="claude" title="Claude Code" form={ClaudeProfileForm} />
