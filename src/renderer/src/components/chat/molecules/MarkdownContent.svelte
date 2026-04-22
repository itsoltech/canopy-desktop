<script lang="ts">
  import { marked } from 'marked'
  import DOMPurify from 'dompurify'

  interface Props {
    content: string
    animateLastToken?: boolean
  }

  let { content, animateLastToken = false }: Props = $props()

  let rootEl = $state<HTMLElement | null>(null)

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

  $effect(() => {
    if (!rootEl) return

    void html

    for (const link of rootEl.querySelectorAll('a[href]')) {
      link.setAttribute('target', '_blank')
      link.setAttribute('rel', 'noreferrer')
    }

    if (animateLastToken) animateLastTextToken(rootEl)
  })

  function animateLastTextToken(root: HTMLElement): void {
    const textNode = lastVisibleTextNode(root)
    if (!textNode?.nodeValue) return

    const value = textNode.nodeValue
    const match = value.match(/(\S+\s*)$/)
    if (!match?.[0]) return

    const token = match[0]
    const tokenStart = value.length - token.length
    const before = document.createTextNode(value.slice(0, tokenStart))
    const tokenNode = document.createElement('span')
    tokenNode.className = 'streaming-token'
    tokenNode.textContent = token

    textNode.replaceWith(before, tokenNode)
  }

  function lastVisibleTextNode(root: HTMLElement): Text | null {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT)
    let latest: Text | null = null

    while (walker.nextNode()) {
      const node = walker.currentNode as Text
      if (node.nodeValue && node.nodeValue.length > 0) latest = node
    }

    return latest
  }
</script>

<div bind:this={rootEl} class="markdown-content" data-message-selectable="true">
  <!-- eslint-disable-next-line svelte/no-at-html-tags -->
  {@html html}
</div>

<style>
  .markdown-content {
    white-space: normal;
    color: inherit;
    min-width: 0;
    font-family: inherit;
    font-size: inherit;
    -webkit-user-select: text;
    user-select: text;
    cursor: text;
  }

  .markdown-content :global(*) {
    box-sizing: border-box;
    -webkit-user-select: text;
    user-select: text;
  }

  .markdown-content :global(p) {
    margin: 0 0 7px;
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
    padding-left: 18px;
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
    font-weight: 600;
    line-height: 1.25;
  }

  .markdown-content :global(h1:first-child),
  .markdown-content :global(h2:first-child),
  .markdown-content :global(h3:first-child),
  .markdown-content :global(h4:first-child) {
    margin-top: 0;
  }

  .markdown-content :global(h1) {
    font-size: 1.12em;
  }

  .markdown-content :global(h2) {
    font-size: 1.05em;
  }

  .markdown-content :global(h3),
  .markdown-content :global(h4) {
    font-size: 1em;
  }

  .markdown-content :global(a) {
    color: var(--c-accent-text);
    text-decoration: none;
  }

  .markdown-content :global(a:hover) {
    text-decoration: underline;
  }

  .markdown-content :global(code) {
    padding: 0 2px;
    border-radius: 0;
    background: transparent;
    border: 0;
    font-family: var(--font-mono, monospace);
    font-size: 1em;
    color: var(--c-accent-text);
  }

  .markdown-content :global(pre) {
    margin: 0 0 8px;
    padding: 8px 10px;
    border-radius: 0;
    border: 1px solid var(--c-border-subtle);
    border-left: 2px solid var(--c-accent-muted);
    background: color-mix(in srgb, var(--c-bg) 86%, black);
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
    margin: 0 0 8px;
    padding: 2px 0 2px 10px;
    border-left: 2px solid var(--c-border);
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
    margin: 0 0 8px;
    border-collapse: collapse;
    overflow-x: auto;
  }

  .markdown-content :global(th),
  .markdown-content :global(td) {
    padding: 4px 6px;
    border: 1px solid var(--c-border-subtle);
    text-align: left;
    vertical-align: top;
  }

  .markdown-content :global(th) {
    background: var(--c-bg-input);
    font-weight: 650;
  }

  .markdown-content :global(.streaming-token) {
    display: inline-block;
    animation: streaming-token-fade 50ms ease-out both;
    will-change: opacity;
  }

  @keyframes streaming-token-fade {
    from {
      opacity: 0;
    }

    to {
      opacity: 1;
    }
  }
</style>
