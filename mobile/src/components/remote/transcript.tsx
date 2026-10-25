import React, { useState } from 'react'
import { View } from 'react-native'
import { useRouter } from 'expo-router'
import { ThemedText } from '@/components/themed-text'
import { useTheme } from '@/hooks/use-theme'
import { Spacing } from '@/constants/theme'
import { Action } from './common'
import { ChatPage } from './chat-page'
import type { HostSession } from '@/lib/remote/grpc/hosts'
import type { TranscriptItem, ChatRef } from '@/lib/remote/grpc/schema'
const kinds: Record<number, string> = { 0: 'Tool', 1: 'Read', 2: 'Edit', 3: 'Execute', 4: 'Search', 5: 'Fetch', 6: 'Plan', 7: 'Subagent', 8: 'Generic tool' }
export function Transcript({ item, host, chat }: { item: TranscriptItem; host: HostSession; chat: ChatRef }) {
  const theme = useTheme(), router = useRouter(), [expanded, setExpanded] = useState(false)
  const tool = item.tool
  const body = item.user?.draft?.text ?? item.text?.text ?? item.thinking?.text ?? item.decision?.title ?? item.notice?.text ?? item.goal?.objective ?? item.turn_end?.outcome ?? ''
  return <View style={{ gap: Spacing.two }}>
    {!!body && <ThemedText selectable themeColor={item.thinking ? 'textSecondary' : 'text'}>{body}</ThemedText>}
    {item.trimmed && <ThemedText type="small">Saved content is incomplete.</ThemedText>}
    {item.shell && <><ThemedText type="code" selectable>{`$ ${item.shell.command}`}</ThemedText><ThemedText>{item.shell.running ? 'Running' : item.shell.stopped ? 'Stopped' : `Exit ${item.shell.exit_code ?? 'unknown'}`}</ThemedText><ThemedText type="code" selectable>{item.shell.output}</ThemedText></>}
    {tool && <View style={{ backgroundColor: theme.backgroundElement, padding: Spacing.three, gap: Spacing.two }}>
      <ThemedText>{kinds[tool.kind] ?? 'Tool'} · {tool.title || tool.original_name} · {tool.state}</ThemedText>
      {!!tool.activity && <ThemedText>{tool.activity}</ThemedText>}
      {tool.elapsed_seconds !== undefined && <ThemedText type="small">{tool.elapsed_seconds}s{tool.background ? ' · Background' : ''}</ThemedText>}
      {!!tool.command && <ThemedText selectable type="code">{tool.command}</ThemedText>}
      {tool.plan.map((entry, index) => <ThemedText key={index}>{entry.status} · {entry.content}{entry.priority ? ` · ${entry.priority}` : ''}</ThemedText>)}
      {tool.files.map(file => <React.Fragment key={`${file.workspace?.workspace_id}:${file.relative_path}`}><Action title={`Open ${file.relative_path}`} run={() => router.push({ pathname: '/remote/workspace', params: { hostId: chat.host_id, workspaceId: file.workspace!.workspace_id, filePath: file.relative_path } })} /><Action title={`Diff ${file.relative_path}`} run={() => router.push({ pathname: '/remote/workspace', params: { hostId: chat.host_id, workspaceId: file.workspace!.workspace_id, filePath: file.relative_path, showDiff: 'true' } })} /></React.Fragment>)}
      {tool.paths.filter(path => !tool.files.some(file => file.relative_path === path)).map(path => <ThemedText key={path} selectable type="small">{path}</ThemedText>)}
      {!!tool.diff.length && <><ThemedText>Changes</ThemedText>{tool.diff.map((line, index) => <ThemedText key={index} selectable type="code">{`${line.old_line ?? ''} ${line.new_line ?? ''} ${line.kind} ${line.text}`}</ThemedText>)}</>}
      {(tool.output || tool.generic_input) && <Action title={expanded ? 'Hide tool details' : 'Show tool details'} run={() => setExpanded(!expanded)} />}
      {expanded && <><ThemedText selectable type="code">{tool.output}</ThemedText>{tool.generic_input && <ThemedText selectable type="code">{tool.generic_input}</ThemedText>}</>}
      {tool.page_item && <ChatPage host={host} chat={chat} item={tool.page_item} />}
    </View>}
    {item.turn_end?.error && <ThemedText>{item.turn_end.error}</ThemedText>}
  </View>
}
