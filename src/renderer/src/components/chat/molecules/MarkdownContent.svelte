<script lang="ts">
  import { marked } from 'marked'
  import DOMPurify from 'dompurify'

  interface Props {
    content: string
  }

  let { content }: Props = $props()

  let html = $derived.by(() => {
    if (!content.trim()) return ''
    return DOMPurify.sanitize(
      marked.parse(content, {
        async: false,
        breaks: true,
        gfm: true,
      }),
    )
  })

  function htmlContent(node: HTMLElement, value: () => string): void {
    $effect(() => {
      node.innerHTML = value()
      for (const link of node.querySelectorAll('a[href]')) {
        link.setAttribute('target', '_blank')
        link.setAttribute('rel', 'noreferrer')
      }
    })
  }
</script>

<div class="markdown-content" use:htmlContent={() => html}></div>

<style>
  .markdown-content {
    white-space: normal;
    color: inherit;
    min-width: 0;
  }

  .markdown-content :global(*) {
    box-sizing: border-box;
  }

  .markdown-content :global(p) {
    margin: 0 0 9px;
  }

  .markdown-content :global(p:last-child),
  .markdown-content :global(ul:last-child),
  .markdown-content :global(ol:last-child),
  .markdown-content :global(pre:last-child),
  .markdown-content :global(blockquote:last-child),
  .markdown-content :global(table:last-child) {
    margin-bottom: 0;
  }

  .markdown-content :global(ul),
  .markdown-content :global(ol) {
    margin: 0 0 9px;
    padding-left: 20px;
  }

  .markdown-content :global(li) {
    margin: 2px 0;
  }

  .markdown-content :global(li > p) {
    margin: 0;
  }

  .markdown-content :global(h1),
  .markdown-content :global(h2),
  .markdown-content :global(h3),
  .markdown-content :global(h4) {
    margin: 12px 0 6px;
    color: var(--c-text);
    font-weight: 650;
    line-height: 1.25;
  }

  .markdown-content :global(h1:first-child),
  .markdown-content :global(h2:first-child),
  .markdown-content :global(h3:first-child),
  .markdown-content :global(h4:first-child) {
    margin-top: 0;
  }

  .markdown-content :global(h1) {
    font-size: 17px;
  }

  .markdown-content :global(h2) {
    font-size: 15.5px;
  }

  .markdown-content :global(h3),
  .markdown-content :global(h4) {
    font-size: 14px;
  }

  .markdown-content :global(a) {
    color: var(--c-accent-text);
    text-decoration: none;
  }

  .markdown-content :global(a:hover) {
    text-decoration: underline;
  }

  .markdown-content :global(code) {
    padding: 1px 4px;
    border-radius: 4px;
    background: var(--c-bg-input);
    border: 1px solid var(--c-border-subtle);
    font-family: var(--font-mono, monospace);
    font-size: 0.92em;
  }

  .markdown-content :global(pre) {
    margin: 0 0 10px;
    padding: 10px 12px;
    border-radius: 6px;
    border: 1px solid var(--c-border-subtle);
    background: var(--c-bg-input);
    overflow-x: auto;
    white-space: pre;
  }

  .markdown-content :global(pre code) {
    padding: 0;
    border: 0;
    background: transparent;
    white-space: inherit;
  }

  .markdown-content :global(blockquote) {
    margin: 0 0 10px;
    padding: 2px 0 2px 10px;
    border-left: 3px solid var(--c-border);
    color: var(--c-text-muted);
  }

  .markdown-content :global(hr) {
    margin: 12px 0;
    border: 0;
    border-top: 1px solid var(--c-border-subtle);
  }

  .markdown-content :global(table) {
    display: block;
    width: 100%;
    margin: 0 0 10px;
    border-collapse: collapse;
    overflow-x: auto;
  }

  .markdown-content :global(th),
  .markdown-content :global(td) {
    padding: 5px 7px;
    border: 1px solid var(--c-border-subtle);
    text-align: left;
    vertical-align: top;
  }

  .markdown-content :global(th) {
    background: var(--c-bg-input);
    font-weight: 650;
  }
</style>
