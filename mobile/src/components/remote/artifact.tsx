import React, { useEffect, useRef, useState } from 'react'
import { MediaPreview } from './media'
import { ThemedText } from '@/components/themed-text'
import { Action } from './common'
import { downloadArtifact, attachmentText } from '@/lib/remote/grpc/artifact-preview'
import type { HostSession } from '@/lib/remote/grpc/hosts'
import type { ArtifactRef } from '@/lib/remote/grpc/schema'
import { fromBase64 } from '@/lib/remote/grpc/protobuf'
export function ArtifactPreview({ host, artifact }: { host: HostSession; artifact: ArtifactRef }) {
  const [html, setHtml] = useState<string>(), [uri, setUri] = useState(''), [text, setText] = useState(''), id = useRef<string | undefined>(undefined), abort = useRef(new AbortController())
  useEffect(() => () => { abort.current.abort(); if (id.current) void host.transport.native.removeAttachment(id.current).catch(() => {}) }, [host])
  return <><Action title={`Preview ${artifact.media_type} · ${Math.ceil(Number(artifact.size) / 1024)} KiB`} disabled={!!uri} run={async () => {
    const result = await downloadArtifact(host, artifact, abort.current.signal); id.current = result.id
    if (abort.current.signal.aborted) { await host.transport.native.removeAttachment(result.id); return }
    if (artifact.media_type === 'text/html') setHtml(await attachmentText(host, result.id, Number(artifact.size)))
    else if (artifact.media_type.startsWith('text/') && Number(artifact.size) <= 1024 * 1024) {
      const parts: Uint8Array[] = []; for (let offset = 0; offset < Number(artifact.size); offset += 65536) parts.push(fromBase64(await host.transport.native.readAttachment(result.id, offset, Math.min(65536, Number(artifact.size) - offset))))
      const bytes = new Uint8Array(Number(artifact.size)); let offset = 0; for (const part of parts) { bytes.set(part, offset); offset += part.length }; setText(new TextDecoder().decode(bytes))
    }
    setUri(result.uri)
  }} />{uri && !text && <MediaPreview uri={uri} mediaType={artifact.media_type} html={html} />}{!!text && <ThemedText selectable type="code">{text}</ThemedText>}</>
}
