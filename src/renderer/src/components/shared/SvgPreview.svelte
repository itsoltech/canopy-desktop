<script lang="ts">
  let { svg, size = 20 }: { svg: string; size?: number } = $props()
</script>

<span class="svg-preview" style:width="{size}px" style:height="{size}px">
  <!-- eslint-disable-next-line svelte/no-at-html-tags -- Single point for rendering user-provided SVG from local disk. Safe: content is read from files the user explicitly selected via OS file picker. -->
  {@html svg}
</span>

<style>
  .svg-preview {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    overflow: hidden;
  }

  .svg-preview :global(svg) {
    width: 100%;
    height: 100%;
  }
</style>
