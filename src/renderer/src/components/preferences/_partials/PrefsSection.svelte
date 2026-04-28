<script lang="ts">
  import type { Snippet } from 'svelte'

  interface Props {
    title: string
    description?: string
    children: Snippet
  }

  let { title, description, children }: Props = $props()
</script>

<section class="flex flex-col gap-3">
  <header class="flex flex-col gap-1">
    <h3
      class="text-2xs font-semibold uppercase tracking-caps-looser text-text-faint m-0 leading-tight"
    >
      {title}
    </h3>
    {#if description}
      <p class="text-xs text-text-muted leading-snug m-0 max-w-[60ch]">{description}</p>
    {/if}
  </header>
  <div class="flex flex-col">
    {@render children()}
  </div>
</section>
