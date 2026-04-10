<script lang="ts">
  let { extra }: { extra: Record<string, unknown> } = $props()

  let cwd = $derived((extra.cwd as string | undefined) ?? null)
  let turnCount = $derived((extra.turnCount as number | undefined) ?? null)
  let transcriptPath = $derived((extra.transcriptPath as string | undefined) ?? null)
  let lastMessage = $derived((extra.lastAssistantMessage as string | undefined) ?? null)

  function shortPath(p: string): string {
    const home = p.match(/^(?:\/Users\/[^/]+|\/home\/[^/]+|[A-Z]:\\Users\\[^\\]+)/)
    if (home) return '~' + p.slice(home[0].length)
    return p
  }

  function truncate(text: string, max: number): string {
    return text.length > max ? text.slice(0, max - 1) + '\u2026' : text
  }
</script>

{#if cwd || turnCount || transcriptPath || lastMessage}
  <div class="section">
    <h4 class="section-label">Codex</h4>
    <div class="info-grid">
      {#if turnCount}
        <span class="info-key">Turns</span>
        <span class="info-val">{turnCount}</span>
      {/if}
      {#if cwd}
        <span class="info-key">CWD</span>
        <span class="info-val mono" title={cwd}>{shortPath(cwd)}</span>
      {/if}
      {#if transcriptPath}
        <span class="info-key">Transcript</span>
        <span class="info-val mono" title={transcriptPath}
          >{shortPath(transcriptPath).split(/[/\\]/).pop()}</span
        >
      {/if}
    </div>
  </div>
{/if}

{#if lastMessage}
  <div class="section">
    <h4 class="section-label">Last response</h4>
    <p class="last-msg">{truncate(lastMessage, 200)}</p>
  </div>
{/if}

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .section-label {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--c-text-faint);
    margin: 0;
  }

  .info-grid {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 3px 12px;
    font-size: 12px;
  }

  .info-key {
    color: var(--c-text-muted);
  }

  .info-val {
    color: var(--c-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mono {
    font-family: monospace;
    font-size: 11px;
  }

  .last-msg {
    font-size: 11px;
    color: var(--c-text-secondary);
    margin: 0;
    line-height: 1.4;
    word-break: break-word;
  }
</style>
