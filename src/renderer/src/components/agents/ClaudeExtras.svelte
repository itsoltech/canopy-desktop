<script lang="ts">
  import { match } from 'ts-pattern'

  let { extra }: { extra: Record<string, unknown> } = $props()

  let rateLimitFiveHour = $derived((extra.rateLimitFiveHour as number | null) ?? null)
  let rateLimitSevenDay = $derived((extra.rateLimitSevenDay as number | null) ?? null)
  let rateLimitFiveHourResetsAt = $derived(
    (extra.rateLimitFiveHourResetsAt as number | null) ?? null,
  )
  let rateLimitSevenDayResetsAt = $derived(
    (extra.rateLimitSevenDayResetsAt as number | null) ?? null,
  )
  let rateLimitSpend = $derived((extra.rateLimitSpend as number | null) ?? null)
  let rateLimitSpendResetsAt = $derived((extra.rateLimitSpendResetsAt as number | null) ?? null)
  let rateLimitSpendUsedUsd = $derived((extra.rateLimitSpendUsedUsd as number | null) ?? null)
  let rateLimitSpendLimitUsd = $derived((extra.rateLimitSpendLimitUsd as number | null) ?? null)
  let rateLimitSpendLabel = $derived(
    match(extra.rateLimitSpendPeriod)
      .with('daily', () => 'Daily spend')
      .with('weekly', () => 'Weekly spend')
      .with('monthly', () => 'Monthly spend')
      .otherwise(() => 'Spend limit'),
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

  function formatSpend(pct: number, usedUsd: number | null, limitUsd: number | null): string {
    if (usedUsd != null && limitUsd != null) {
      return `$${usedUsd.toFixed(2)} / $${limitUsd.toFixed(2)}`
    }
    // Remaining, like the windows above. used_percentage passes 100 once exceeded.
    return `${Math.max(Math.round(100 - pct), 0)}%`
  }

  function rateLimitBarClass(pct: number): string {
    if (pct >= 90) return 'bg-danger'
    if (pct >= 70) return 'bg-warning'
    return 'bg-success'
  }
</script>

{#if rateLimitFiveHour != null || rateLimitSevenDay != null || rateLimitSpend != null}
  <div class="flex flex-col gap-1.5">
    <h4 class="text-2xs font-semibold tracking-[0.5px] uppercase text-text-faint m-0">
      Rate Limits
    </h4>
    {#if rateLimitFiveHour != null}
      <div class="flex flex-col gap-[3px]">
        <div class="flex justify-between items-baseline">
          <span class="text-sm text-text-muted">5h window</span>
          <span class="text-xs text-text-secondary"
            >{Math.round(100 - rateLimitFiveHour)}%{#if formatResetTime(rateLimitFiveHourResetsAt)}
              <span class="text-text-faint ml-[0.3em]"
                >{formatResetTime(rateLimitFiveHourResetsAt)}</span
              >{/if}</span
          >
        </div>
        <div class="h-1 rounded-xs bg-active overflow-hidden">
          <div
            class="h-full rounded-xs transition-[width] duration-slow {rateLimitBarClass(
              rateLimitFiveHour,
            )}"
            style="width: {Math.max(100 - rateLimitFiveHour, 0)}%"
          ></div>
        </div>
      </div>
    {/if}
    {#if rateLimitSevenDay != null}
      <div class="flex flex-col gap-[3px]">
        <div class="flex justify-between items-baseline">
          <span class="text-sm text-text-muted">7d window</span>
          <span class="text-xs text-text-secondary"
            >{Math.round(100 - rateLimitSevenDay)}%{#if formatResetTime(rateLimitSevenDayResetsAt)}
              <span class="text-text-faint ml-[0.3em]"
                >{formatResetTime(rateLimitSevenDayResetsAt)}</span
              >{/if}</span
          >
        </div>
        <div class="h-1 rounded-xs bg-active overflow-hidden">
          <div
            class="h-full rounded-xs transition-[width] duration-slow {rateLimitBarClass(
              rateLimitSevenDay,
            )}"
            style="width: {Math.max(100 - rateLimitSevenDay, 0)}%"
          ></div>
        </div>
      </div>
    {/if}
    {#if rateLimitSpend != null}
      <div class="flex flex-col gap-[3px]">
        <div class="flex justify-between items-baseline">
          <span class="text-sm text-text-muted">{rateLimitSpendLabel}</span>
          <span class="text-xs text-text-secondary"
            >{formatSpend(
              rateLimitSpend,
              rateLimitSpendUsedUsd,
              rateLimitSpendLimitUsd,
            )}{#if formatResetTime(rateLimitSpendResetsAt)}
              <span class="text-text-faint ml-[0.3em]"
                >{formatResetTime(rateLimitSpendResetsAt)}</span
              >{/if}</span
          >
        </div>
        <div class="h-1 rounded-xs bg-active overflow-hidden">
          <div
            class="h-full rounded-xs transition-[width] duration-slow {rateLimitBarClass(
              rateLimitSpend,
            )}"
            style="width: {Math.max(100 - rateLimitSpend, 0)}%"
          ></div>
        </div>
      </div>
    {/if}
  </div>
{/if}
