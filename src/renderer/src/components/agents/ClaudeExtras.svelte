<script lang="ts">
  let { extra }: { extra: Record<string, unknown> } = $props()

  let rateLimitFiveHour = $derived((extra.rateLimitFiveHour as number | null) ?? null)
  let rateLimitSevenDay = $derived((extra.rateLimitSevenDay as number | null) ?? null)
  let rateLimitFiveHourResetsAt = $derived(
    (extra.rateLimitFiveHourResetsAt as number | null) ?? null,
  )
  let rateLimitSevenDayResetsAt = $derived(
    (extra.rateLimitSevenDayResetsAt as number | null) ?? null,
  )

  function formatResetTime(resetsAt: number | null): string {
    if (resetsAt == null) return ''
    const diff = resetsAt - Date.now()
    if (diff <= 0) return ''
    const minutes = Math.floor(diff / 60000)
    if (minutes < 1) return 'in <1min'
    const hours = Math.floor(minutes / 60)
    const mins = minutes % 60
    const days = Math.floor(hours / 24)
    const remHours = hours % 24
    if (days > 0) {
      if (remHours === 0 && mins === 0) return `in ${days}d`
      if (mins === 0) return `in ${days}d ${remHours}h`
      return `in ${days}d ${remHours}h ${mins}min`
    }
    if (hours === 0) return `in ${mins}min`
    if (mins === 0) return `in ${hours}h`
    return `in ${hours}h ${mins}min`
  }

  function rateLimitBarClass(pct: number): string {
    if (pct >= 90) return 'ctx-red'
    if (pct >= 70) return 'ctx-yellow'
    return 'ctx-green'
  }
</script>

{#if rateLimitFiveHour != null || rateLimitSevenDay != null}
  <div class="section">
    <h4 class="section-label">Rate Limits</h4>
    {#if rateLimitFiveHour != null}
      <div class="rate-limit-row">
        <div class="rate-limit-header">
          <span class="rate-limit-label">5h window</span>
          <span class="rate-limit-meta"
            >{Math.round(100 - rateLimitFiveHour)}%{#if formatResetTime(rateLimitFiveHourResetsAt)}
              <span class="rate-limit-reset">{formatResetTime(rateLimitFiveHourResetsAt)}</span
              >{/if}</span
          >
        </div>
        <div class="context-track">
          <div
            class="context-fill {rateLimitBarClass(rateLimitFiveHour)}"
            style="width: {Math.max(100 - rateLimitFiveHour, 0)}%"
          ></div>
        </div>
      </div>
    {/if}
    {#if rateLimitSevenDay != null}
      <div class="rate-limit-row">
        <div class="rate-limit-header">
          <span class="rate-limit-label">7d window</span>
          <span class="rate-limit-meta"
            >{Math.round(100 - rateLimitSevenDay)}%{#if formatResetTime(rateLimitSevenDayResetsAt)}
              <span class="rate-limit-reset">{formatResetTime(rateLimitSevenDayResetsAt)}</span
              >{/if}</span
          >
        </div>
        <div class="context-track">
          <div
            class="context-fill {rateLimitBarClass(rateLimitSevenDay)}"
            style="width: {Math.max(100 - rateLimitSevenDay, 0)}%"
          ></div>
        </div>
      </div>
    {/if}
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
    color: var(--color-text-faint);
    margin: 0;
  }

  .context-track {
    height: 4px;
    border-radius: 2px;
    background: var(--color-active);
    overflow: hidden;
  }

  .context-fill {
    height: 100%;
    border-radius: 2px;
    transition: width 0.3s ease;
  }

  .context-fill.ctx-green {
    background: var(--color-success);
  }

  .context-fill.ctx-yellow {
    background: var(--color-warning);
  }

  .context-fill.ctx-red {
    background: var(--color-danger);
  }

  .rate-limit-row {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .rate-limit-header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }

  .rate-limit-label {
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .rate-limit-meta {
    font-size: 11px;
    color: var(--color-text-secondary);
  }

  .rate-limit-reset {
    color: var(--color-text-faint);
    margin-left: 0.3em;
  }
</style>
