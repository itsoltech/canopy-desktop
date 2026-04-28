<script lang="ts">
  let { extra }: { extra: Record<string, unknown> } = $props()

  let pendingQuestion = $derived((extra.pendingQuestion as string | null) ?? null)
</script>

{#if pendingQuestion}
  <div class="flex flex-col gap-1.5">
    <h4 class="text-2xs font-semibold tracking-[0.5px] uppercase text-text-faint m-0">
      Waiting for input
    </h4>
    <div class="px-2.5 py-1.5 rounded-lg border-l-[3px] border-warning-text bg-border-subtle">
      <span class="text-sm text-text-secondary leading-snug">{pendingQuestion}</span>
    </div>
  </div>
{/if}
