import React, { useState } from 'react'
import { Action, Field } from './common'
import { ArtifactPreview } from './artifact'
import { ThemedText } from '@/components/themed-text'
import type { HostSession } from '@/lib/remote/grpc/hosts'
import type { ThreadRef } from '@/lib/remote/grpc/schema'
import { command, checkReceipt } from '@/lib/remote/grpc/commands'
export function EditMessage({ host, chat, chatId, runId }: { host: HostSession; chat: ThreadRef; chatId: string; runId: string }) {
  const [edit, setEdit] = useState(host.store.snapshot().edits![chatId]), [failure, setFailure] = useState('')
  const save = async (next: typeof edit) => {
    setEdit(next)
    await host.store.update(state => { (state.edits ??= {})[chatId] = next; return null })
  }
  return <><ThemedText>Edit sent message{edit.preview.first_message ? ' · First message' : ''}</ThemedText>
    <ThemedText type="small">The original conversation history is retained. Your separate unsent composer draft is unchanged.</ThemedText>
    <Field label="Replacement message" multiline value={edit.draft.text} onChangeText={text => { void save({ ...edit, draft: { ...edit.draft, text } }).catch(error => setFailure(String(error))) }} />
    {edit.draft.attachments.map((attachment, index) => <React.Fragment key={index}>{attachment.image ? <ArtifactPreview host={host} artifact={attachment.image} /> : <ThemedText>{attachment.label}</ThemedText>}<Action title={`Remove ${attachment.label}`} run={() => save({ ...edit, draft: { ...edit.draft, attachments: edit.draft.attachments.filter((_, i) => i !== index) } })} /></React.Fragment>)}
    {!!edit.preview.files?.paths.length && <><ThemedText>File rewind preview · +{edit.preview.files.insertions} / −{edit.preview.files.deletions}</ThemedText>{edit.preview.files.paths.map(path => <ThemedText key={path}>{path}</ThemedText>)}</>}
    {!edit.preview.files?.possible && <ThemedText>{edit.preview.files?.reason || 'File rewind is unavailable for this message.'}</ThemedText>}
    <Action title={`${edit.restoreFiles ? '✓ ' : ''}Restore files before editing`} disabled={!edit.preview.files?.possible} run={() => save({ ...edit, restoreFiles: !edit.restoreFiles })} />
    <Action title="Refresh edit preview" run={async () => {
      const preview = await host.transport.unary('ThreadService/PreviewEdit', { thread: chat, run_id: runId, item: edit.preview.item })
      if (!preview.editable) throw Error(preview.reason)
      await save({ ...edit, preview, restoreFiles: false })
    }} />
    {!!failure && <ThemedText>{failure}</ThemedText>}
    <Action title={edit.restoreFiles ? 'Restore files and send replacement' : 'Send replacement'} disabled={!edit.draft.text.trim() && !edit.draft.attachments.length} run={async () => {
      await save(edit)
      try {
        const receipt = await host.mutate('ThreadService/EditMessage', { command: command(host.store), target: await host.threadTarget(chat), run_id: edit.preview.run_id, item: edit.preview.item, draft: edit.draft, restore_files: edit.restoreFiles })
        if (receipt.error) throw Error([receipt.error.code, receipt.error.message].filter(Boolean).join('\n'))
        checkReceipt(receipt)
        if (receipt.state === 3) await host.store.update(state => { if (JSON.stringify(state.edits?.[chatId]?.draft) === JSON.stringify(edit.draft)) delete state.edits?.[chatId]; return null })
        else setFailure('Edit accepted; query its result before another submission. The replacement draft is retained.')
      } catch (error) { setFailure(`${String(error)}\nReplacement retained. Refresh the preview for changed conditions. If files were not restored, turn off file restoration before a new explicit attempt.`); throw error }
    }} />
    <Action title="Cancel editing" run={() => host.store.update(state => { delete state.edits?.[chatId]; return null })} />
  </>
}
