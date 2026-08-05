<script lang="ts">
  import type { CiJobStatus } from '../../lib/ci/types'
  import { ciLastRunTimestamp, ciRunChip, ciRunStatusTextClass } from '../../lib/ci/status'
  import CiLastStatusCard from './CiLastStatusCard.svelte'

  let { rows, branch }: { rows: CiJobStatus[]; branch: string } = $props()
  let statusRows = $derived(
    rows.map((row) => ({
      id: row.jobId,
      label: row.label,
      number: row.run?.number,
      webUrl: row.run?.webUrl,
      timestamp: row.run ? ciLastRunTimestamp(row.run) : undefined,
      statusText: row.run?.statusText,
      statusTextClass: row.run ? ciRunStatusTextClass(row.run) : undefined,
      error: row.error,
      chip: ciRunChip(row),
    })),
  )
</script>

<CiLastStatusCard
  rows={statusRows}
  {branch}
  providerLabel="GitHub Actions"
  cardTitle="Last run"
  runNoun="run"
/>
