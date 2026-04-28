<script lang="ts">
  interface Props {
    total: number
    current: number
  }

  let { total, current }: Props = $props()
</script>

<div class="flex gap-2 justify-center" role="group" aria-label="Step {current + 1} of {total}">
  {#each Array.from({ length: total }, (_, k) => k) as i (i)}
    <div
      class="w-2 h-2 rounded-full bg-text-faint transition-[background,transform] duration-base"
      class:!bg-accent={i === current}
      class:scale-125={i === current}
      class:!bg-accent-muted={i < current}
    ></div>
  {/each}
</div>
