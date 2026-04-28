<script lang="ts">
  import { Layers } from '@lucide/svelte'
</script>

<div class="flex flex-col items-center text-center gap-3 py-6">
  <div class="w-20 h-20 rounded-[20px] bg-accent-bg flex items-center justify-center mb-2">
    <Layers size={48} strokeWidth={1.5} color="var(--color-accent)" />
  </div>
  <h2 class="m-0 text-[22px] font-semibold text-text">Welcome to Canopy</h2>
  <p class="m-0 text-lg text-text-secondary leading-normal max-w-[380px]">
    A developer workstation for managing multiple projects, branches, and AI assistants in parallel.
  </p>
  <p class="mt-2 mb-0 text-sm text-text-faint">
    This setup takes about a minute. You can change everything later in Preferences.
  </p>
</div>
