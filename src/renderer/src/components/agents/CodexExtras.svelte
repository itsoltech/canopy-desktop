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
    return text.length > max ? text.slice(0, max - 1) + '…' : text
  }
</script>

{#if cwd || turnCount || transcriptPath || lastMessage}
  <div class="flex flex-col gap-1.5">
    <h4 class="text-2xs font-semibold tracking-[0.5px] uppercase text-text-faint m-0">Codex</h4>
    <div class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-[3px] text-sm">
      {#if turnCount}
        <span class="text-text-muted">Turns</span>
        <span class="text-text-secondary overflow-hidden text-ellipsis whitespace-nowrap"
          >{turnCount}</span
        >
      {/if}
      {#if cwd}
        <span class="text-text-muted">CWD</span>
        <span
          class="text-text-secondary overflow-hidden text-ellipsis whitespace-nowrap font-mono text-xs"
          title={cwd}>{shortPath(cwd)}</span
        >
      {/if}
      {#if transcriptPath}
        <span class="text-text-muted">Transcript</span>
        <span
          class="text-text-secondary overflow-hidden text-ellipsis whitespace-nowrap font-mono text-xs"
          title={transcriptPath}>{shortPath(transcriptPath).split(/[/\\]/).pop()}</span
        >
      {/if}
    </div>
  </div>
{/if}

{#if lastMessage}
  <div class="flex flex-col gap-1.5">
    <h4 class="text-2xs font-semibold tracking-[0.5px] uppercase text-text-faint m-0">
      Last response
    </h4>
    <p class="text-xs text-text-secondary m-0 leading-snug break-words">
      {truncate(lastMessage, 200)}
    </p>
  </div>
{/if}
