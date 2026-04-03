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

<div class="step">
  <h2 class="title">Choose your tools</h2>
  <p class="description">
    Select the AI assistants you plan to use. Canopy will check if they are installed.
  </p>

  <div class="toggles">
    {#each tools as tool (tool.id)}
      <label class="toggle-row">
        <CustomCheckbox
          checked={onboardingState.selectedTools.has(tool.id)}
          onchange={() => toggle(tool.id)}
        />
        <div class="toggle-info">
          <span class="toggle-label">{tool.name}</span>
          <span class="toggle-hint">{tool.hint}</span>
        </div>
      </label>
    {/each}
  </div>
</div>

<style>
  .step {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 16px;
  }

  .title {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    color: var(--c-text);
  }

  .description {
    margin: 0;
    font-size: 13px;
    color: var(--c-text-secondary);
  }

  .toggles {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    max-width: 400px;
    text-align: left;
  }

  .toggle-row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 8px;
    transition: background 0.1s;
  }

  .toggle-row:hover {
    background: var(--c-border-subtle);
  }

  .toggle-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .toggle-label {
    font-size: 13px;
    color: var(--c-text);
  }

  .toggle-hint {
    font-size: 11px;
    color: var(--c-text-muted);
    line-height: 1.4;
  }
</style>
