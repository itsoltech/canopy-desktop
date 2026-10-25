import { useTheme } from '@/hooks/use-theme'
import { requireNativeViewManager } from 'expo-modules-core'
import type { ViewProps } from 'react-native'
import type { KeyModifiers } from '@/lib/remote/grpc/terminal-keys'
import React, { useEffect, useMemo, useState } from 'react'
import { Alert, ScrollView, Text, View } from 'react-native'
import { useLocalSearchParams } from 'expo-router'
import { Action, Field, Page, useHost, useRevision } from '@/components/remote/common'
import { ThemedText } from '@/components/themed-text'
import { Fonts } from '@/constants/theme'
import type { HostSession } from '@/lib/remote/grpc/hosts'
import { TerminalViewOwner } from '@/lib/remote/grpc/terminal'
import { TerminalCellFlag as Flag, TerminalInputMode } from '@/lib/remote/grpc/schema'
import { command, checkReceipt } from '@/lib/remote/grpc/commands'
const HardwareKeyboard = requireNativeViewManager<ViewProps & { onTerminalKey: (event: { nativeEvent: KeyModifiers & { key: string } }) => void }>('CanopyRemote')
const rgba = (n: number) => `#${(n >>> 0).toString(16).padStart(8, '0')}`
export default function TerminalScreen() {
  const { hostId, terminalId } = useLocalSearchParams<{ hostId: string; terminalId: string }>(), { host, error } = useHost(hostId)
  return host ? <Terminal host={host} terminalId={terminalId} /> : <Page title="Terminal"><ThemedText>{error ?? 'Loading…'}</ThemedText></Page>
}
function Terminal({ host, terminalId }: { host: HostSession; terminalId: string }) {
  const owner = useMemo(() => new TerminalViewOwner(host, terminalId), [host, terminalId]); useRevision(owner)
  useEffect(() => { owner.start(); return () => { void owner.dispose().catch(() => {}) } }, [owner])
  const state = owner.snapshot(), [text, setText] = useState(''), [columns, setColumns] = useState('80'), [rows, setRows] = useState('24')
  const [modifiers, setModifiers] = useState({ ctrl: false, alt: false, shift: false, meta: false })
  const theme = useTheme()
  const display = state.history?.frame ?? state.frame
  const cells = new Map(display?.cells.map(c => [`${c.row}:${c.column}`, c]))
  return <Page key={`${host?.snapshot().hostId}:terminal:${terminalId}`} store={host?.store} viewKey={`terminal:${terminalId}`} title="Terminal"><ThemedText>{state.error ?? (state.lease?.held ? 'Input owned by this device' : 'Read only')}</ThemedText>{state.frame?.lost_scrollback && <ThemedText>Earlier scrollback is unavailable.</ThemedText>}
    <ScrollView horizontal><View>{Array.from({ length: display?.rows ?? 0 }, (_, row) => <Text key={row} selectable style={{ fontFamily: Fonts.mono, fontSize: 12, lineHeight: 16 }}>{Array.from({ length: display?.columns ?? 0 }, (_, column) => { const c = cells.get(`${row}:${column}`); const cursor = !state.history && display?.cursor_row === row && display.cursor_column === column && (BigInt(display.input_modes) & BigInt(TerminalInputMode.CURSOR_VISIBLE)) !== 0n; return <Text key={column} style={c ? { color: rgba((!!(c.flags & Flag.CELL_INVERSE) !== cursor) ? c.background_rgba : c.foreground_rgba), backgroundColor: rgba((!!(c.flags & Flag.CELL_INVERSE) !== cursor) ? c.foreground_rgba : c.background_rgba), fontWeight: c.flags & Flag.CELL_BOLD ? '700' : '400', fontStyle: c.flags & Flag.CELL_ITALIC ? 'italic' : 'normal', opacity: c.flags & Flag.CELL_DIM ? 0.6 : 1, textDecorationLine: c.flags & Flag.CELL_UNDERLINE ? 'underline' : c.flags & Flag.CELL_STRIKEOUT ? 'line-through' : 'none' } : cursor ? { color: theme.background, backgroundColor: theme.text } : undefined}>{c?.column_span === 0 ? '' : c?.flags && c.flags & Flag.CELL_HIDDEN ? ' '.repeat(c.column_span || 1) : c?.text || ' '}</Text> })}</Text>)}</View></ScrollView>
    <Action title="Read scrollback" run={() => owner.earlier(200)} />{state.history?.start.previous_page_token && <Action title="Earlier scrollback" run={() => owner.earlier(0, true)} />}<Action title="Live terminal" run={() => owner.live()} />

    <Action title="Acquire input" run={() => owner.acquire(false)} /><Action title="Take over input" run={() => Alert.alert('Take over terminal?', 'The other device will lose input control.', [{ text: 'Cancel' }, { text: 'Take over', onPress: () => { void owner.acquire(true).catch(e => Alert.alert('Could not acquire', String(e))) } }])} /><Action title="Release input" run={() => owner.release()} />
    {state.lease?.held && <><ThemedText type="small">Tap below to focus a hardware keyboard. Use the input field for composed text.</ThemedText><HardwareKeyboard style={{ height: 48 }} onTerminalKey={event => { const { key, ...modifiers } = event.nativeEvent; void owner.key(key, modifiers).catch(error => Alert.alert('Key not sent', String(error))) }} /></>}
    <Field label="Terminal input" multiline value={text} onChangeText={setText} /><Action title="Send modified character" disabled={!state.lease?.held || text.length !== 1} run={() => owner.key(text, modifiers)} /><Action title="Send input" disabled={!state.lease?.held || !!state.pendingInput} run={async () => { const data = text; await owner.input(data); setText(current => current === data ? '' : current) }} />
    <Action title="Paste composer text" disabled={!state.lease?.held} run={async () => { const data = text; await owner.paste(data); setText(current => current === data ? '' : current) }} />
    {(['ctrl', 'alt', 'shift', 'meta'] as const).map(modifier => <Action key={modifier} title={`${modifiers[modifier] ? '✓ ' : ''}${modifier}`} run={() => setModifiers(current => ({ ...current, [modifier]: !current[modifier] }))} />)}
    {(['up', 'down', 'left', 'right', 'enter', 'escape', 'backspace', 'delete', 'insert', 'home', 'end', 'pageup', 'pagedown', 'tab', 'f1', 'f2', 'f3', 'f4', 'f5', 'f6', 'f7', 'f8', 'f9', 'f10', 'f11', 'f12'] as const).map(key => <Action key={key} title={key} disabled={!state.lease?.held} run={() => owner.key(key, modifiers)} />)}<Action title="Ctrl-C" disabled={!state.lease?.held} run={() => owner.input('\u0003')} /><Action title="Tab" disabled={!state.lease?.held} run={() => owner.input('\t')} />
    <Field label="Columns" keyboardType="number-pad" value={columns} onChangeText={setColumns} /><Field label="Rows" keyboardType="number-pad" value={rows} onChangeText={setRows} /><Action title="Resize" disabled={!state.lease?.held} run={() => { const c = Number(columns), r = Number(rows); if (!Number.isInteger(c) || !Number.isInteger(r) || c < 1 || r < 1 || c * r > 200000) throw Error('Invalid terminal size'); return owner.resize(c, r) }} />
    <Action title="Reconnect terminal" run={() => owner.open()} />{(['StopTerminal', 'RestartTerminal'] as const).map(method => <Action key={method} title={method === 'StopTerminal' ? 'Stop' : 'Restart'} run={async () => checkReceipt(await host.mutate(`TerminalService/${method}`, { command: command(host.store), terminal: owner.terminal, run_id: state.runId }))} />)}
  </Page>
}
