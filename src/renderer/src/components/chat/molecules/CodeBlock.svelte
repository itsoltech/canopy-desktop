<script lang="ts">
  import CopyButton from '../atoms/CopyButton.svelte'

  interface Props {
    code: string
    language?: string
    filename?: string
  }

  let { code, language, filename }: Props = $props()

  let label = $derived(filename ?? language ?? 'code')
</script>

<figure class="code-block">
  <header class="code-head">
    <span class="code-label">{label}</span>
    <CopyButton text={code} size={12} />
  </header>
  <pre class="code-body"><code>{code}</code></pre>
</figure>

<style>
  .code-block {
    display: flex;
    flex-direction: column;
    margin: 6px 0;
    border: 1px solid var(--c-border-subtle);
    border-radius: 6px;
    overflow: hidden;
    background: var(--c-bg);
  }

  .code-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 4px 6px 4px 10px;
    border-bottom: 1px solid var(--c-border-subtle);
    background: var(--c-bg-elevated);
  }

  .code-label {
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 11px;
    color: var(--c-text-muted);
    text-transform: lowercase;
  }

  .code-body {
    margin: 0;
    padding: 10px 12px;
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 12px;
    line-height: 1.55;
    color: var(--c-text);
    overflow-x: auto;
    white-space: pre;
    -webkit-user-select: text;
    user-select: text;
  }

  .code-body code {
    font: inherit;
    color: inherit;
    background: transparent;
  }
</style>
