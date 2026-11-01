import React, { useState } from 'react'
import { Action, Field } from './common'
import { ThemedText } from '@/components/themed-text'
import { command, checkReceipt } from '@/lib/remote/grpc/commands'
import type { HostSession } from '@/lib/remote/grpc/hosts'
import type { ThreadRef, Decision, NativeDecision } from '@/lib/remote/grpc/schema'

/** A decision lives only with its run/id. Unmount and disconnect never answer it. */
export function DecisionForm({ host, chat, decision, ready }: { host: HostSession; chat: ThreadRef; decision: Decision; ready: boolean }) {
  const [chosen, setChosen] = useState<Record<number, string[]>>({})
  const [typed, setTyped] = useState<Record<number, string>>({})
  const [note, setNote] = useState(''), [acceptEdits, setAcceptEdits] = useState(false)
  const answer = async (native: NativeDecision) => checkReceipt(await host.mutate('ThreadService/RespondDecision', {
    command: command(host.store), target: await host.threadTarget(chat), run_id: decision.run_id, decision_id: decision.decision_id, native,
  }))
  const respond = (action: NativeDecision['action']) => answer({ action, answers: [], note: note || undefined, accept_edits: acceptEdits })
  const answers = decision.questions.map((q, i) => {
    const text = typed[i]?.trim() ?? '', picks = chosen[i] ?? []
    return q.multiple ? [...picks, ...(text ? [text] : [])].join(', ') : text || picks.join(', ')
  })
  return <>
    {decision.description && <ThemedText selectable>{decision.description}</ThemedText>}
    {decision.options.map(option => <Action key={option.id} title={option.label} disabled={!ready} run={async () => checkReceipt(await host.mutate('ThreadService/RespondDecision', { command: command(host.store), target: await host.threadTarget(chat), run_id: decision.run_id, decision_id: decision.decision_id, option_id: option.id }))} />)}
    {decision.native_kind === 'question' && <>
      {decision.questions.map((q, i) => <React.Fragment key={`${decision.decision_id}:${i}`}>
        <ThemedText>{q.header ? `${q.header}: ` : ''}{q.question}</ThemedText>
        {q.options.map(option => <Action key={option.id} title={`${chosen[i]?.includes(option.label) ? '✓ ' : ''}${option.label}${option.description ? ` — ${option.description}` : ''}`} disabled={!ready} run={() => setChosen(previous => ({ ...previous, [i]: q.multiple ? previous[i]?.includes(option.label) ? previous[i].filter(v => v !== option.label) : [...(previous[i] ?? []), option.label] : [option.label] }))} />)}
        <Field label="Your answer" value={typed[i] ?? ''} onChangeText={text => setTyped(previous => ({ ...previous, [i]: text }))} />
      </React.Fragment>)}
      <Action title="Submit answers" disabled={!ready || !answers.length || answers.some(a => !a)} run={() => answer({ action: 4, answers, accept_edits: false })} />
      <Action title="Decline" disabled={!ready} run={() => respond(3)} />
    </>}
    {decision.native_kind === 'tool' && <>
      <Action title="Allow once" disabled={!ready} run={() => respond(1)} />
      {decision.can_remember && <Action title="Allow for this chat" disabled={!ready} run={() => respond(2)} />}
      <Field label="Reason for denial (optional)" value={note} onChangeText={setNote} />
      <Action title="Deny" disabled={!ready} run={() => respond(3)} />
    </>}
    {decision.native_kind === 'plan' && <>
      <Action title={`${acceptEdits ? '✓ ' : ''}Accept edits after approval`} disabled={!ready} run={() => setAcceptEdits(!acceptEdits)} />
      <Field label="Plan feedback (optional)" multiline value={note} onChangeText={setNote} />
      <Action title="Approve plan" disabled={!ready} run={() => respond(5)} />
      <Action title="Keep planning" disabled={!ready} run={() => respond(6)} />
    </>}
  </>
}
