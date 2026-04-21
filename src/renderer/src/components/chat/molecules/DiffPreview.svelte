<script lang="ts">
  interface Props {
    oldText?: string
    newText?: string
    maxLines?: number
  }

  let { oldText, newText, maxLines = 20 }: Props = $props()

  let oldLines = $derived(oldText !== undefined ? oldText.split('\n') : [])
  let newLines = $derived(newText !== undefined ? newText.split('\n') : [])

  let hasOld = $derived(oldText !== undefined && oldText.length > 0)
  let hasNew = $derived(newText !== undefined && newText.length > 0)

  let oldShown = $derived(oldLines.slice(0, maxLines))
  let newShown = $derived(newLines.slice(0, maxLines))

  let oldHidden = $derived(Math.max(0, oldLines.length - maxLines))
  let newHidden = $derived(Math.max(0, newLines.length - maxLines))

  let stats = $derived.by(() => {
    const adds = hasNew ? newLines.length : 0
    const dels = hasOld ? oldLines.length : 0
    return { adds, dels }
  })
</script>

<div class="diff">
  <header class="diff-head">
    {#if stats.dels > 0}
      <span class="stat dels">-{stats.dels}</span>
    {/if}
    {#if stats.adds > 0}
      <span class="stat adds">+{stats.adds}</span>
    {/if}
    {#if !hasOld && !hasNew}
      <span class="stat empty">empty</span>
    {/if}
  </header>

  <div class="diff-body">
    {#if hasOld}
      {#each oldShown as line, i (`d${i}`)}
        <div class="line removed">
          <span class="marker">-</span><span class="code">{line || ' '}</span>
        </div>
      {/each}
      {#if oldHidden > 0}
        <div class="line truncated">… {oldHidden} more line{oldHidden === 1 ? '' : 's'}</div>
      {/if}
    {/if}

    {#if hasNew}
      {#each newShown as line, i (`a${i}`)}
        <div class="line added">
          <span class="marker">+</span><span class="code">{line || ' '}</span>
        </div>
      {/each}
      {#if newHidden > 0}
        <div class="line truncated">… {newHidden} more line{newHidden === 1 ? '' : 's'}</div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .diff {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--c-border-subtle);
    border-radius: 4px;
    background: var(--c-bg);
    overflow: hidden;
  }

  .diff-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 8px;
    background: var(--c-bg-elevated);
    border-bottom: 1px solid var(--c-border-subtle);
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 10.5px;
    font-weight: 600;
  }

  .stat.dels {
    color: var(--diff-delete-fg);
  }

  .stat.adds {
    color: var(--diff-add-fg);
  }

  .stat.empty {
    color: var(--c-text-muted);
    font-weight: 400;
  }

  .diff-body {
    overflow-x: auto;
    padding: 4px 0;
  }

  .line {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 0 8px;
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 11.5px;
    line-height: 1.55;
    white-space: pre;
    -webkit-user-select: text;
    user-select: text;
  }

  .line.removed {
    background: var(--diff-delete-bg);
    color: var(--c-text);
  }

  .line.removed .marker {
    color: var(--diff-delete-fg);
  }

  .line.added {
    background: var(--diff-add-bg);
    color: var(--c-text);
  }

  .line.added .marker {
    color: var(--diff-add-fg);
  }

  .line.truncated {
    background: transparent;
    color: var(--c-text-faint);
    font-style: italic;
    padding-left: 26px;
  }

  .marker {
    flex-shrink: 0;
    width: 10px;
    font-weight: 600;
    -webkit-user-select: none;
    user-select: none;
  }

  .code {
    flex: 1;
  }
</style>
