<script lang="ts">
  import { ciChip, ciLastRunTimestampInfo, ciStatusTextClass } from '../../lib/ci/status'
  import type { CiBuildTypeStatus } from '../../lib/ci/types'
  import CiLastStatusCard from './CiLastStatusCard.svelte'

  let { rows, branch }: { rows: CiBuildTypeStatus[]; branch: string } = $props()
  let statusRows = $derived(
    rows.map((row) => {
      const timestamp = row.build ? ciLastRunTimestampInfo(row.build) : undefined
      return {
        id: row.buildTypeId,
        label: row.label,
        number: row.build?.number,
        webUrl: row.build?.webUrl,
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

<CiLastStatusCard
  rows={statusRows}
  {branch}
  providerLabel="TeamCity"
  cardTitle="Last job"
  runNoun="build"
/>
