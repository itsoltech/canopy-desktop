<script lang="ts">
  interface Props {
    input?: Record<string, unknown>
    result?: string
  }

  let { input, result }: Props = $props()

  function str(...keys: string[]): string | undefined {
    if (!input) return undefined
    for (const key of keys) {
      const value = input[key]
      if (typeof value === 'string' && value.trim().length > 0) return value
    }
    return undefined
  }

  let skillName = $derived(str('skill', 'name') ?? 'unknown')
  let statusText = $derived(result?.trim() || `Launching skill: ${skillName}`)
</script>

<div class="skill-view">
  <div class="skill-line">
    <span class="skill-label">Skill</span>
    <span class="skill-name">{skillName}</span>
  </div>

  <div class="status-line">{statusText}</div>
</div>

<style>
  .skill-view {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .skill-line {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .skill-label {
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--c-text-muted);
    font-weight: 600;
  }

  .skill-name {
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 12px;
    padding: 1px 6px;
    border-radius: 3px;
    background: var(--c-bg-elevated);
    color: var(--c-text);
  }

  .status-line {
    padding: 6px 10px;
    border: 1px solid var(--c-border-subtle);
    border-radius: 4px;
    background: var(--c-bg);
    font-size: 12px;
    line-height: 1.45;
    color: var(--c-text-secondary);
  }
</style>
