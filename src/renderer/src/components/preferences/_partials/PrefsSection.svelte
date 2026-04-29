<script lang="ts">
  import type { Snippet } from 'svelte'

  type BadgeTone = 'warning' | 'accent' | 'neutral' | 'danger'

  interface Props {
    title: string
    description?: string
    badge?: { text: string; tone?: BadgeTone }
    children: Snippet
  }

  let { title, description, badge, children }: Props = $props()

  const badgeClasses: Record<BadgeTone, string> = {
    warning: 'bg-experimental-bg text-warning-text border border-experimental-border',
    accent: 'bg-accent-bg text-accent-text',
    neutral: 'bg-border-subtle text-text-muted',
    danger: 'bg-danger-bg text-danger-text',
  }
</script>

<section class="flex flex-col gap-3">
  <header class="flex flex-col gap-1">
    <div class="flex items-center gap-2">
      <h3
        class="text-2xs font-semibold uppercase tracking-caps-looser text-text-faint m-0 leading-tight"
      >
        {title}
      </h3>
      {#if badge}
        <span
          class="inline-flex items-center text-2xs font-semibold uppercase tracking-caps-tight px-1.5 py-px rounded-md leading-tight {badgeClasses[
            badge.tone ?? 'neutral'
          ]}"
        >
          {badge.text}
        </span>
      {/if}
    </div>
    {#if description}
      <p class="text-xs text-text-muted leading-snug m-0 max-w-[60ch]">{description}</p>
    {/if}
  </header>
  <div class="flex flex-col">
    {@render children()}
  </div>
</section>
