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
    font-size: inherit;
    font-weight: 500;
    letter-spacing: 0;
    padding: 0;
    border-radius: 0;
    background: transparent;
    color: var(--c-text-secondary);
    line-height: 1.25;
  }

  .role-badge::before {
    content: '[';
    color: var(--c-text-faint);
  }

  .role-badge::after {
    content: ']';
    color: var(--c-text-faint);
  }

  .role-badge.user {
    background: transparent;
    color: var(--c-accent-text);
  }

  .role-badge.assistant {
    background: transparent;
    color: var(--c-success);
  }

  .role-badge.tool {
    background: transparent;
    color: var(--c-generate);
  }

  .role-badge.system {
    background: transparent;
    color: var(--c-warning);
  }
</style>
