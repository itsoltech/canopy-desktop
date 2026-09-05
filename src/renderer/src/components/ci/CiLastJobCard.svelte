<script lang="ts">
  import {
    anyBuildActive,
    ciChip,
    ciLastRunTimestampInfo,
    ciStatusTextClass,
    newestLastStatusIndex,
  } from '../../lib/ci/status'
  import type { CiBuildTypeStatus, CiCardIssue, CiLastStatusRow } from '../../lib/ci/types'
  import CiLastStatusCard from './CiLastStatusCard.svelte'

  let {
    rows,
    branch,
    issue,
    onActivate,
  }: {
    rows: CiBuildTypeStatus[]
    branch: string
    issue?: CiCardIssue
    onActivate: () => void
  } = $props()

  let statusRows = $derived(
    rows.map((row): CiLastStatusRow => {
      const timestamp = row.build ? ciLastRunTimestampInfo(row.build) : undefined
      return {
        id: row.buildTypeId,
        label: row.label,
        number: row.build?.number,
        timestamp: timestamp?.value,
        timestampLabel: timestamp?.label,
        statusText: row.build?.statusText,
        statusTextClass: row.build ? ciStatusTextClass(row.build) : undefined,
        error: row.error,
        chip: ciChip(row),
      }
    }),
  )
  // Index rather than the row itself: it keeps the mapped row and its source aligned, so
  // "is it running" is answered by the build actually on screen, not by any configured job.
  let index = $derived(newestLastStatusIndex(statusRows))
  let row = $derived(index === -1 ? undefined : statusRows[index])
  let source = $derived(index === -1 ? undefined : rows[index])
</script>

<CiLastStatusCard
  {row}
  {branch}
  {issue}
  {onActivate}
  active={anyBuildActive(source ? [source] : [])}
/>
