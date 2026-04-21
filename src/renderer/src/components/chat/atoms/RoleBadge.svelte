<script lang="ts">
  type Role = 'user' | 'assistant' | 'tool' | 'system'

  interface Props {
    role: Role
    label?: string
  }

  let { role, label }: Props = $props()

  const defaults: Record<Role, string> = {
    user: 'You',
    assistant: 'Assistant',
    tool: 'Tool',
    system: 'System',
  }

  let text = $derived(label ?? defaults[role])
</script>

<span class="role-badge {role}">{text}</span>

<style>
  .role-badge {
    display: inline-flex;
    align-items: center;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--c-bg-elevated);
    color: var(--c-text-secondary);
    line-height: 1.4;
  }

  .role-badge.user {
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
  }

  .role-badge.assistant {
    background: var(--c-bg-elevated);
    color: var(--c-text);
  }

  .role-badge.tool {
    background: color-mix(in srgb, var(--c-generate) 15%, transparent);
    color: var(--c-generate);
  }

  .role-badge.system {
    background: color-mix(in srgb, var(--c-warning) 15%, transparent);
    color: var(--c-warning);
  }
</style>
