<script lang="ts">
  import { onboardingState } from '../../../lib/stores/onboarding.svelte'
  import CustomCheckbox from '../../shared/CustomCheckbox.svelte'

  const tools = [
    { id: 'claude', name: 'Claude Code', hint: 'Anthropic CLI for AI-assisted coding.' },
    { id: 'codex', name: 'Codex', hint: 'OpenAI coding agent.' },
    { id: 'gemini', name: 'Gemini CLI', hint: 'Google AI assistant for the terminal.' },
  ] as const

  function toggle(id: string): void {
    if (onboardingState.selectedTools.has(id)) {
      onboardingState.selectedTools.delete(id)
    } else {
      onboardingState.selectedTools.add(id)
    }
  }
</script>

<div class="flex flex-col items-center text-center gap-4">
  <h2 class="m-0 text-lg font-semibold text-text">Choose your tools</h2>
  <p class="m-0 text-md text-text-secondary">
    Select the AI assistants you plan to use. Canopy will check if they are installed.
  </p>

  <div class="flex flex-col gap-0.5 w-full max-w-[400px] text-left">
    {#each tools as tool (tool.id)}
      <label
        class="flex items-start gap-2.5 px-3 py-2.5 rounded-xl transition-colors duration-fast hover:bg-border-subtle"
      >
        <CustomCheckbox
          checked={onboardingState.selectedTools.has(tool.id)}
          onchange={() => toggle(tool.id)}
        />
        <div class="flex flex-col gap-0.5">
          <span class="text-md text-text">{tool.name}</span>
          <span class="text-xs text-text-muted leading-snug">{tool.hint}</span>
        </div>
      </label>
    {/each}
  </div>
</div>
