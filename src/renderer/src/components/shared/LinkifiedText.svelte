<script lang="ts">
  import { detectPathsInText, type PathMatch } from '../../lib/pathDetection/linkify'
  import { openFile } from '../../lib/stores/tabs.svelte'
  import { ensureLoaded, getFiles } from '../../lib/stores/quickOpenStore.svelte'

  type Props = {
    text: string
    cwd: string
  }

  let { text, cwd }: Props = $props()

  type Token =
    | { type: 'text'; content: string }
    | { type: 'link'; content: string; absolutePath: string; line?: number }

  $effect(() => {
    if (cwd) void ensureLoaded(cwd)
  })

  const knownFiles = $derived(new Set(getFiles(cwd)))

  function tokenize(raw: string, basePath: string, files: ReadonlySet<string>): Token[] {
    if (!raw) return []
    const matches: PathMatch[] = detectPathsInText(raw, basePath, files)
    if (matches.length === 0) return [{ type: 'text', content: raw }]
    const tokens: Token[] = []
    let cursor = 0
    for (const m of matches) {
      if (m.start > cursor) {
        tokens.push({ type: 'text', content: raw.slice(cursor, m.start) })
      }
      tokens.push({
        type: 'link',
        content: m.raw,
        absolutePath: m.absolutePath,
        line: m.line,
      })
      cursor = m.end
    }
    if (cursor < raw.length) {
      tokens.push({ type: 'text', content: raw.slice(cursor) })
    }
    return tokens
  }

  const tokens = $derived(tokenize(text, cwd, knownFiles))
</script>

{#each tokens as tok, i (i)}
  {#if tok.type === 'link'}
    <button
      type="button"
      class="path-link"
      onclick={() => openFile(tok.absolutePath, cwd, { line: tok.line })}
      title={tok.absolutePath}
    >
      {tok.content}
    </button>
  {:else}{tok.content}{/if}
{/each}

<style>
  .path-link {
    display: inline;
    padding: 0;
    margin: 0;
    background: none;
    border: none;
    color: var(--c-accent, #4a9eff);
    text-decoration: underline;
    cursor: pointer;
    font-family: inherit;
    font-size: inherit;
    line-height: inherit;
  }

  .path-link:hover {
    text-decoration: none;
    background: var(--c-hover);
  }
</style>
