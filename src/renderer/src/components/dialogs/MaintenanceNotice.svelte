<script lang="ts">
  import { ArrowUpRight, TriangleAlert } from '@lucide/svelte'
  import { migrationGuideUrl } from '../../lib/maintenanceNotice'

  const links = [
    { label: 'Migration guide', href: migrationGuideUrl },
    { label: 'Canopy website', href: 'https://canopy.itsol.tech' },
    { label: 'GitHub repository', href: 'https://github.com/itsoltech/canopy' },
  ]

  let { class: className = '' }: { class?: string } = $props()
</script>

<div
  class="flex items-start gap-3 rounded-lg border border-experimental-border bg-experimental-bg p-3 {className}"
>
  <TriangleAlert size={16} class="mt-0.5 shrink-0 text-warning-text" aria-hidden="true" />
  <div class="min-w-0">
    <h3 class="m-0 text-md font-semibold text-text">Canopy Desktop is no longer maintained</h3>
    <p class="mt-1 text-md text-text-secondary">The new Canopy is available now.</p>
    <div class="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-md font-medium">
      {#each links as { label, href } (href)}
        <a
          {href}
          target="_blank"
          rel="noopener noreferrer"
          class="inline-flex items-center gap-1 rounded-sm text-accent-text underline underline-offset-2 hover:text-text focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-2"
        >
          {label}
          <ArrowUpRight size={13} aria-hidden="true" />
        </a>
      {/each}
    </div>
  </div>
</div>
