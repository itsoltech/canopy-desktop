export interface PaneSession {
  id: string
  sessionId: string
  wsUrl: string
  toolId: string
  toolName: string
  isRunning: boolean
  exitCode: number | null
  title: string | null
  paneType?: 'terminal' | 'browser'
  url?: string
  isLoading?: boolean
}

export type SplitNode =
  | { type: 'leaf'; pane: PaneSession }
  | { type: 'hsplit'; id: string; first: SplitNode; second: SplitNode; ratio: number }
  | { type: 'vsplit'; id: string; first: SplitNode; second: SplitNode; ratio: number }

let paneCounter = 0
let splitCounter = 0

export function nextPaneId(): string {
  return `pane-${++paneCounter}`
}

export function nextSplitId(): string {
  return `split-${++splitCounter}`
}

export function createLeaf(pane: PaneSession): SplitNode {
  return { type: 'leaf', pane }
}

export function findLeaf(root: SplitNode, paneId: string): PaneSession | null {
  if (root.type === 'leaf') {
    return root.pane.id === paneId ? root.pane : null
  }
  return findLeaf(root.first, paneId) ?? findLeaf(root.second, paneId)
}

export function allPanes(root: SplitNode): PaneSession[] {
  if (root.type === 'leaf') return [root.pane]
  return [...allPanes(root.first), ...allPanes(root.second)]
}

export function firstLeaf(root: SplitNode): PaneSession {
  if (root.type === 'leaf') return root.pane
  return firstLeaf(root.first)
}

export function depth(node: SplitNode): number {
  if (node.type === 'leaf') return 1
  return 1 + Math.max(depth(node.first), depth(node.second))
}

const MAX_DEPTH = 4

export function splitPane(
  root: SplitNode,
  paneId: string,
  direction: 'hsplit' | 'vsplit',
  newPane: PaneSession,
): SplitNode | null {
  if (depth(root) >= MAX_DEPTH) return null
  return splitPaneInner(root, paneId, direction, newPane)
}

function splitPaneInner(
  node: SplitNode,
  paneId: string,
  direction: 'hsplit' | 'vsplit',
  newPane: PaneSession,
): SplitNode | null {
  if (node.type === 'leaf') {
    if (node.pane.id !== paneId) return null
    return {
      type: direction,
      id: nextSplitId(),
      first: node,
      second: { type: 'leaf', pane: newPane },
      ratio: 0.5,
    }
  }

  const firstResult = splitPaneInner(node.first, paneId, direction, newPane)
  if (firstResult) {
    return { ...node, first: firstResult }
  }

  const secondResult = splitPaneInner(node.second, paneId, direction, newPane)
  if (secondResult) {
    return { ...node, second: secondResult }
  }

  return null
}

export function removePane(
  root: SplitNode,
  paneId: string,
): { tree: SplitNode | null; removed: PaneSession } | null {
  if (root.type === 'leaf') {
    if (root.pane.id === paneId) {
      return { tree: null, removed: root.pane }
    }
    return null
  }

  // Check if target is a direct child
  if (root.first.type === 'leaf' && root.first.pane.id === paneId) {
    return { tree: root.second, removed: root.first.pane }
  }
  if (root.second.type === 'leaf' && root.second.pane.id === paneId) {
    return { tree: root.first, removed: root.second.pane }
  }

  // Recurse into children
  const firstResult = removePane(root.first, paneId)
  if (firstResult) {
    return {
      tree: firstResult.tree ? { ...root, first: firstResult.tree } : root.second,
      removed: firstResult.removed,
    }
  }

  const secondResult = removePane(root.second, paneId)
  if (secondResult) {
    return {
      tree: secondResult.tree ? { ...root, second: secondResult.tree } : root.first,
      removed: secondResult.removed,
    }
  }

  return null
}

export function updatePane(
  root: SplitNode,
  paneId: string,
  updater: (pane: PaneSession) => PaneSession,
): SplitNode {
  if (root.type === 'leaf') {
    if (root.pane.id === paneId) {
      return { type: 'leaf', pane: updater(root.pane) }
    }
    return root
  }
  return {
    ...root,
    first: updatePane(root.first, paneId, updater),
    second: updatePane(root.second, paneId, updater),
  }
}

export function findSplitRatio(root: SplitNode, splitId: string): number | null {
  if (root.type === 'leaf') return null
  if (root.id === splitId) return root.ratio
  return findSplitRatio(root.first, splitId) ?? findSplitRatio(root.second, splitId)
}

export function updateRatio(root: SplitNode, splitId: string, ratio: number): SplitNode {
  if (root.type === 'leaf') return root
  if (root.id === splitId) {
    return { ...root, ratio }
  }
  return {
    ...root,
    first: updateRatio(root.first, splitId, ratio),
    second: updateRatio(root.second, splitId, ratio),
  }
}

export function graftSubtree(
  root: SplitNode,
  targetPaneId: string,
  direction: 'hsplit' | 'vsplit',
  subtree: SplitNode,
  position: 'first' | 'second',
): SplitNode | null {
  const subtreeDepth = depth(subtree)
  return graftInner(root, targetPaneId, direction, subtree, position, subtreeDepth, 1)
}

function graftInner(
  node: SplitNode,
  targetPaneId: string,
  direction: 'hsplit' | 'vsplit',
  subtree: SplitNode,
  position: 'first' | 'second',
  subtreeDepth: number,
  currentDepth: number,
): SplitNode | null {
  if (node.type === 'leaf') {
    if (node.pane.id !== targetPaneId) return null
    // New split adds 1 level, plus the subtree depth
    if (currentDepth + subtreeDepth > MAX_DEPTH) return null
    const first = position === 'first' ? subtree : node
    const second = position === 'first' ? node : subtree
    return {
      type: direction,
      id: nextSplitId(),
      first,
      second,
      ratio: 0.5,
    }
  }

  const firstResult = graftInner(
    node.first,
    targetPaneId,
    direction,
    subtree,
    position,
    subtreeDepth,
    currentDepth + 1,
  )
  if (firstResult) {
    return { ...node, first: firstResult }
  }

  const secondResult = graftInner(
    node.second,
    targetPaneId,
    direction,
    subtree,
    position,
    subtreeDepth,
    currentDepth + 1,
  )
  if (secondResult) {
    return { ...node, second: secondResult }
  }

  return null
}

// --- Layout computation (flat rects for absolute positioning) ---

export interface PaneRect {
  paneId: string
  pane: PaneSession
  x: number
  y: number
  w: number
  h: number
}

export interface DividerRect {
  splitId: string
  direction: 'horizontal' | 'vertical'
  x: number
  y: number
  w: number
  h: number
}

export interface FlatLayout {
  panes: PaneRect[]
  dividers: DividerRect[]
}

// Divider gap in normalized units — will be computed relative to container size
const DIVIDER_PX = 4

export function buildFlatLayout(
  root: SplitNode,
  containerWidth = 1,
  containerHeight = 1,
): FlatLayout {
  const panes: PaneRect[] = []
  const dividers: DividerRect[] = []
  // Normalize divider size to fraction of container
  const gapX = containerWidth > 0 ? DIVIDER_PX / containerWidth : 0
  const gapY = containerHeight > 0 ? DIVIDER_PX / containerHeight : 0
  buildLayoutInner(root, 0, 0, 1, 1, gapX, gapY, panes, dividers)
  return { panes, dividers }
}

function buildLayoutInner(
  node: SplitNode,
  x: number,
  y: number,
  w: number,
  h: number,
  gapX: number,
  gapY: number,
  panes: PaneRect[],
  dividers: DividerRect[],
): void {
  if (node.type === 'leaf') {
    panes.push({ paneId: node.pane.id, pane: node.pane, x, y, w, h })
    return
  }

  if (node.type === 'vsplit') {
    const splitPos = w * node.ratio
    const firstW = Math.max(0, splitPos - gapX / 2)
    const secondX = x + splitPos + gapX / 2
    const secondW = Math.max(0, w - splitPos - gapX / 2)
    buildLayoutInner(node.first, x, y, firstW, h, gapX, gapY, panes, dividers)
    buildLayoutInner(node.second, secondX, y, secondW, h, gapX, gapY, panes, dividers)
    dividers.push({
      splitId: node.id,
      direction: 'vertical',
      x: x + splitPos - gapX / 2,
      y,
      w: gapX,
      h,
    })
  } else {
    const splitPos = h * node.ratio
    const firstH = Math.max(0, splitPos - gapY / 2)
    const secondY = y + splitPos + gapY / 2
    const secondH = Math.max(0, h - splitPos - gapY / 2)
    buildLayoutInner(node.first, x, y, w, firstH, gapX, gapY, panes, dividers)
    buildLayoutInner(node.second, x, secondY, w, secondH, gapX, gapY, panes, dividers)
    dividers.push({
      splitId: node.id,
      direction: 'horizontal',
      x,
      y: y + splitPos - gapY / 2,
      w,
      h: gapY,
    })
  }
}

// Navigation helpers

interface LeafRect {
  paneId: string
  x: number
  y: number
  w: number
  h: number
}

function buildLeafRects(node: SplitNode, x: number, y: number, w: number, h: number): LeafRect[] {
  if (node.type === 'leaf') {
    return [{ paneId: node.pane.id, x, y, w, h }]
  }

  if (node.type === 'vsplit') {
    const firstW = w * node.ratio
    const secondW = w - firstW
    return [
      ...buildLeafRects(node.first, x, y, firstW, h),
      ...buildLeafRects(node.second, x + firstW, y, secondW, h),
    ]
  }

  // hsplit
  const firstH = h * node.ratio
  const secondH = h - firstH
  return [
    ...buildLeafRects(node.first, x, y, w, firstH),
    ...buildLeafRects(node.second, x, y + firstH, w, secondH),
  ]
}

export function navigateFrom(
  root: SplitNode,
  fromPaneId: string,
  direction: 'left' | 'right' | 'up' | 'down',
): string | null {
  const rects = buildLeafRects(root, 0, 0, 1, 1)
  const source = rects.find((r) => r.paneId === fromPaneId)
  if (!source) return null

  const srcCx = source.x + source.w / 2
  const srcCy = source.y + source.h / 2
  const eps = 0.001

  const candidates = rects.filter((r) => {
    if (r.paneId === fromPaneId) return false
    const cx = r.x + r.w / 2
    const cy = r.y + r.h / 2

    switch (direction) {
      case 'right':
        return cx > srcCx + eps
      case 'left':
        return cx < srcCx - eps
      case 'down':
        return cy > srcCy + eps
      case 'up':
        return cy < srcCy - eps
    }
  })

  if (candidates.length === 0) return null

  // Sort by distance to source center
  candidates.sort((a, b) => {
    const aCx = a.x + a.w / 2
    const aCy = a.y + a.h / 2
    const bCx = b.x + b.w / 2
    const bCy = b.y + b.h / 2
    const distA = Math.abs(aCx - srcCx) + Math.abs(aCy - srcCy)
    const distB = Math.abs(bCx - srcCx) + Math.abs(bCy - srcCy)
    return distA - distB
  })

  return candidates[0].paneId
}
