<script lang="ts">
  import { ciChip, ciLastRunTimestamp } from '../../lib/ci/status'
  import type { CiBuildTypeStatus } from '../../lib/ci/types'
  import CiLastStatusCard from './CiLastStatusCard.svelte'

  let { rows, branch }: { rows: CiBuildTypeStatus[]; branch: string } = $props()
  let statusRows = $derived(
    rows.map((row) => ({
      id: row.buildTypeId,
      label: row.label,
      number: row.build?.number,
      webUrl: row.build?.webUrl,
      timestamp: row.build ? ciLastRunTimestamp(row.build) : undefined,
      error: row.error,
      chip: ciChip(row),
    })),
  )
</script>

<CiLastStatusCard
  rows={statusRows}
  {branch}
  providerLabel="TeamCity"
  cardTitle="Last job"
  runNoun="build"
/>
