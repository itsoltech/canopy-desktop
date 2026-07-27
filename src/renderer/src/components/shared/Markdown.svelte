<script lang="ts">
  import { marked } from 'marked'
  import DOMPurify from 'dompurify'

  // The one markdown renderer for the app: task descriptions/comments, PR bodies,
  // notes preview, changelog, license. Parsing is async (marked may return a
  // promise) and generation-guarded so a slower parse of an older source can never
  // overwrite a newer one. Output is DOMPurify-sanitized; link clicks need no local
  // handling — the main process blocks top-level navigation (`will-navigate`) and
  // routes safe external URLs to the OS browser.
  let { source = '', class: cls = '' }: { source?: string; class?: string } = $props()

  let html = $state('')
  let parseGen = 0

  $effect(() => {
    const gen = ++parseGen
    const raw = source
    if (!raw.trim()) {
      html = ''
      return
    }
    // GFM with newline-as-break matches how trackers (GitHub, YouTrack, Jira
    // comments) treat single newlines in descriptions and comments.
    Promise.resolve(marked.parse(raw, { gfm: true, breaks: true })).then((parsed) => {
      if (gen !== parseGen) return
      html = DOMPurify.sanitize(parsed)
    })
  })
</script>

<div class="md-content {cls}">
  <!-- eslint-disable-next-line svelte/no-at-html-tags -- sanitized above -->
  {@html html}
</div>

<style>
  /* Em-based so the consumer's text size utility scales the whole document. */
  .md-content :global(> :first-child) {
    margin-top: 0;
  }
  .md-content :global(> :last-child) {
    margin-bottom: 0;
  }
  .md-content :global(h1),
  .md-content :global(h2),
  .md-content :global(h3),
  .md-content :global(h4),
  .md-content :global(h5),
  .md-content :global(h6) {
    margin: 0.9em 0 0.35em;
    font-weight: 600;
    line-height: 1.3;
    color: var(--color-text);
  }
  .md-content :global(h1) {
    font-size: 1.35em;
  }
  .md-content :global(h2) {
    font-size: 1.2em;
  }
  .md-content :global(h3) {
    font-size: 1.1em;
  }
  .md-content :global(h4),
  .md-content :global(h5),
  .md-content :global(h6) {
    font-size: 1em;
  }
  .md-content :global(p) {
    margin: 0.45em 0;
    line-height: 1.5;
  }
  .md-content :global(ul),
  .md-content :global(ol) {
    margin: 0.45em 0;
    padding-left: 1.4em;
  }
  .md-content :global(li) {
    margin: 0.15em 0;
    line-height: 1.5;
  }
  .md-content :global(li > ul),
  .md-content :global(li > ol) {
    margin: 0.1em 0;
  }
  .md-content :global(a) {
    color: var(--color-accent-text);
    text-decoration: none;
  }
  .md-content :global(a:hover) {
    text-decoration: underline;
  }
  .md-content :global(code) {
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 0.92em;
    background: var(--color-bg-input);
    border: 1px solid var(--color-border-subtle);
    border-radius: 4px;
    padding: 0.05em 0.3em;
  }
  .md-content :global(pre) {
    margin: 0.55em 0;
    padding: 0.6em 0.75em;
    background: var(--color-bg-input);
    border: 1px solid var(--color-border-subtle);
    border-radius: 6px;
    overflow-x: auto;
  }
  .md-content :global(pre code) {
    background: transparent;
    border: 0;
    padding: 0;
    font-size: 0.92em;
  }
  .md-content :global(blockquote) {
    margin: 0.55em 0;
    padding: 0.1em 0.8em;
    border-left: 3px solid var(--color-border);
    color: var(--color-text-muted);
  }
  .md-content :global(hr) {
    border: 0;
    border-top: 1px solid var(--color-border-subtle);
    margin: 0.8em 0;
  }
  .md-content :global(table) {
    border-collapse: collapse;
    margin: 0.55em 0;
    display: block;
    max-width: 100%;
    overflow-x: auto;
  }
  .md-content :global(th),
  .md-content :global(td) {
    border: 1px solid var(--color-border-subtle);
    padding: 0.25em 0.6em;
    text-align: left;
  }
  .md-content :global(th) {
    background: var(--color-bg-input);
    font-weight: 600;
  }
  .md-content :global(img) {
    max-width: 100%;
    border-radius: 6px;
  }
  .md-content :global(input[type='checkbox']) {
    margin-right: 0.35em;
  }
</style>
