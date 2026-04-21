<script lang="ts">
  import FilePath from '../atoms/FilePath.svelte'
  import DiffPreview from './DiffPreview.svelte'

  interface Props {
    input?: Record<string, unknown>
    result?: string
  }

  let { input, result }: Props = $props()

  function str(...keys: string[]): string | undefined {
    if (!input) return undefined
    for (const k of keys) {
      const v = input[k]
      if (typeof v === 'string') return v
    }
    return undefined
  }

  let path = $derived(str('file_path', 'filePath', 'path') ?? '')
  let oldText = $derived(str('old_string', 'oldString'))
  // Write/create uses `content` instead of new_string.
  let newText = $derived(str('new_string', 'newString', 'content'))
</script>

<div class="edit-view">
  {#if path}
    <div class="row">
      <FilePath {path} />
    </div>
  {/if}
  <DiffPreview {oldText} {newText} />
  {#if result}
    <div class="result-line">{result}</div>
  {/if}
</div>

<style>
  .edit-view {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .result-line {
    font-size: 11.5px;
    color: var(--c-text-muted);
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    -webkit-user-select: text;
    user-select: text;
  }
</style>
