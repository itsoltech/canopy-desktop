import type { Stroke } from '../../lib/stores/drawings.svelte'
import { getActiveAgentPane } from '../../lib/stores/tabs.svelte'
import { addToast } from '../../lib/stores/toast.svelte'
import { blobFromCanvas } from './drawingCanvas'

export function deleteSelected(strokes: Stroke[], selectedIds: Set<string>): Stroke[] {
  if (selectedIds.size === 0) return strokes
  const filtered = strokes.filter((s) => !selectedIds.has(s.id))
  selectedIds.clear()
  return filtered
}

export function selectAll(strokes: Stroke[], selectedIds: Set<string>): void {
  for (const s of strokes) selectedIds.add(s.id)
}

export function undoLast(strokes: Stroke[], selectedIds: Set<string>): Stroke[] {
  if (strokes.length === 0) return strokes
  const last = strokes[strokes.length - 1]
  selectedIds.delete(last.id)
  return strokes.slice(0, -1)
}

export async function exportPng(
  canvas: HTMLCanvasElement,
  strokes: Stroke[],
  selectedIds: Set<string>,
  doRedraw: () => void,
): Promise<Blob | null> {
  if (strokes.length === 0) {
    addToast('Drawing is empty')
    return null
  }
  if (selectedIds.size === 0) return blobFromCanvas(canvas)

  const snapshot = [...selectedIds]
  selectedIds.clear()
  doRedraw()
  try {
    return await blobFromCanvas(canvas)
  } finally {
    for (const id of snapshot) selectedIds.add(id)
  }
}

export async function sendToAgent(
  canvas: HTMLCanvasElement,
  strokes: Stroke[],
  selectedIds: Set<string>,
  doRedraw: () => void,
): Promise<void> {
  const blob = await exportPng(canvas, strokes, selectedIds, doRedraw)
  if (!blob) return

  const agent = getActiveAgentPane()
  if (!agent) {
    addToast('Open a Claude/Codex pane first')
    return
  }
  if (!agent.isRunning) {
    addToast('Agent pane is not running')
    return
  }

  try {
    await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })])
  } catch (err) {
    console.error('[drawing] clipboard.write failed:', err)
    addToast('Clipboard copy failed — check permissions')
    return
  }

  // OS pasteboard write is async on macOS; fast follow-up Ctrl+V can race it
  await new Promise((resolve) => setTimeout(resolve, 250))
  await window.api.writePty(agent.sessionId, '\x16')
  addToast(`Sent to ${agent.toolName}`)
}

export async function copyPng(
  canvas: HTMLCanvasElement,
  strokes: Stroke[],
  selectedIds: Set<string>,
  doRedraw: () => void,
): Promise<void> {
  const blob = await exportPng(canvas, strokes, selectedIds, doRedraw)
  if (!blob) return
  try {
    await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })])
    addToast('Copied PNG to clipboard')
  } catch (err) {
    console.error('[drawing] clipboard.write failed:', err)
    addToast('Clipboard copy failed')
  }
}
