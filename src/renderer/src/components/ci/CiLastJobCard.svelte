<script lang="ts">
  import {
    anyBuildActive,
    ciChip,
    ciLastRunTimestampInfo,
    ciStatusTextClass,
  } from '../../lib/ci/status'
  import type { CiBuildTypeStatus, CiCardIssue } from '../../lib/ci/types'
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
    rows.map((row) => {
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
</script>

<CiLastStatusCard rows={statusRows} {branch} {issue} {onActivate} active={anyBuildActive(rows)} />
