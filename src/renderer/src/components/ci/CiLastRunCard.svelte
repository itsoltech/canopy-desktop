<script lang="ts">
  import type { CiCardIssue, CiJobStatus } from '../../lib/ci/types'
  import {
    anyRunActive,
    ciLastRunTimestampInfo,
    ciRunChip,
    ciRunStatusTextClass,
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
    rows.map((row) => {
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
</script>

<CiLastStatusCard rows={statusRows} {branch} {issue} {onActivate} active={anyRunActive(rows)} />
