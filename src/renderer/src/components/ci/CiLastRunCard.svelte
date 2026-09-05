<script lang="ts">
  import type { CiCardIssue, CiJobStatus, CiLastStatusRow } from '../../lib/ci/types'
  import {
    anyRunActive,
    ciLastRunTimestampInfo,
    ciRunChip,
    ciRunStatusTextClass,
    newestLastStatusIndex,
  } from '../../lib/ci/status'
  import CiLastStatusCard from './CiLastStatusCard.svelte'

  let {
    rows,
    branch,
    issue,
    onActivate,
  }: {
    rows: CiJobStatus[]
    branch: string
    issue?: CiCardIssue
    onActivate: () => void
  } = $props()

  let statusRows = $derived(
    rows.map((row): CiLastStatusRow => {
      const timestamp = row.run ? ciLastRunTimestampInfo(row.run) : undefined
      return {
        id: row.jobId,
        label: row.label,
        number: row.run?.number,
        timestamp: timestamp?.value,
        timestampLabel: timestamp?.label,
        statusText: row.run?.statusText,
        statusTextClass: row.run ? ciRunStatusTextClass(row.run) : undefined,
        error: row.error,
        chip: ciRunChip(row),
      }
    }),
  )
  // Index rather than the row itself: it keeps the mapped row and its source aligned, so
  // "is it running" is answered by the run actually on screen, not by any configured job.
  let index = $derived(newestLastStatusIndex(statusRows))
  let row = $derived(index === -1 ? undefined : statusRows[index])
  let source = $derived(index === -1 ? undefined : rows[index])
</script>

<CiLastStatusCard
  {row}
  {branch}
  {issue}
  {onActivate}
  active={anyRunActive(source ? [source] : [])}
/>
