import React, { useEffect, useRef, useState } from 'react'
import { ArtifactPreview } from './artifact'
import { Action } from './common'
import type { HostSession } from '@/lib/remote/grpc/hosts'
import type { TaskAttachment, TaskRef, ArtifactRef } from '@/lib/remote/grpc/schema'
// Resolve the provider attachment to a host artifact through the scoped typed RPC.
// The native artifact renderer subsequently downloads/validates the same host ref.
export function TaskAttachmentPreview({ host, task, attachment }: { host: HostSession; task: TaskRef; attachment: TaskAttachment }) {
  const abort = useRef(new AbortController())
  useEffect(() => () => abort.current.abort(), [])
  const [artifact, setArtifact] = useState<ArtifactRef>()
  return <>{artifact ? <ArtifactPreview host={host} artifact={artifact} /> : <Action title={`Open ${attachment.name}`} run={async () => {
    let first: ArtifactRef | undefined, offset = 0n, complete = false
    const stream = await host.transport.subscribe('TasksService/ReadAttachment', { task, attachment_id: attachment.id }, chunk => {
      if (!chunk.artifact || chunk.artifact.host_id !== task.host_id || complete || chunk.offset !== String(offset)) throw Error('Invalid task attachment stream')
      if (first && first.artifact_id !== chunk.artifact.artifact_id) throw Error('Attachment changed')
      first = chunk.artifact; offset += BigInt(chunk.data.length); complete = chunk.complete
      if (offset > 64n * 1024n * 1024n) throw Error('Attachment exceeds 64 MiB preview limit')
    }, abort.current.signal)
    await stream.done
    if (!first || !complete || offset !== BigInt(first.size)) throw Error('Incomplete task attachment')
    setArtifact(first)
  }} />}</>
}
