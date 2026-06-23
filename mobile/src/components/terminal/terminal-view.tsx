'use dom'

import '@xterm/xterm/css/xterm.css'

import { FitAddon } from '@xterm/addon-fit'
import { Terminal as XTerm } from '@xterm/xterm'
import { type DOMImperativeFactory, useDOMImperativeHandle } from 'expo/dom'
import { useCallback, useEffect, useRef, useState } from 'react'

import jetBrainsMonoRegular from '../../../assets/fonts/JetBrainsMonoNerdFontMono-Regular.ttf'
import { resolveTerminalPalette, type TerminalPalette } from '../../constants/terminal-themes'
import type { TerminalThemeId } from '../../lib/storage/app-preferences-types'

const PRIMARY_FONT = 'JetBrainsMonoNerdFontMono'
const FONT_STACK = `"${PRIMARY_FONT}", Menlo, Monaco, "SF Mono", monospace`

// Cap how many wheel reports / arrow keys a single touchmove may emit, so a
// fast flick can't flood the WebRTC data channel. Travel beyond the cap is
// dropped for that frame (not re-accumulated) — see onTouchMove.
const MAX_ALT_LINES_PER_MOVE = 8

// Translate an accumulated vertical swipe (in whole terminal rows) into the
// input bytes a full-screen TUI on the alternate buffer expects. Apps drawing
// on the alternate screen (lazygit, htop, the Claude TUI) have no scrollback,
// so term.scrollLines() is a dead no-op there; instead we forward the swipe to
// the PTY as the app's own scroll input. Positive `lines` = finger swiped down
// = reveal older content = wheel-up / arrow-up, matching the scrollLines(-lines)
// convention used on the normal buffer.
function emitAltScrollInput(
  term: XTerm,
  lines: number,
  touchX: number,
  touchY: number,
  host: HTMLElement,
  emit: (seq: string) => void,
): void {
  const count = Math.min(Math.abs(lines), MAX_ALT_LINES_PER_MOVE)
  if (count === 0) return
  const up = lines > 0

  // Mouse-tracking apps (e.g. lazygit) want SGR wheel reports targeted at the
  // cell under the finger so the correct pane scrolls. We emit SGR (CSI <)
  // encoding unconditionally: the target full-screen TUIs (lazygit, htop, the
  // Claude/Codex agent UIs) all request SGR extended mouse mode (DECSET 1006)
  // alongside tracking. An app that enabled tracking with legacy X10 encoding
  // would mis-parse this, but none of the apps this targets do.
  if (term.modes.mouseTrackingMode !== 'none') {
    const cols = term.cols
    const rows = term.rows
    if (cols <= 0 || rows <= 0) return
    const cellWidth = host.clientWidth / cols
    const cellHeight = host.clientHeight / rows
    if (cellWidth <= 0 || cellHeight <= 0) return
    const rect = host.getBoundingClientRect()
    const col = Math.min(Math.max(Math.ceil((touchX - rect.left) / cellWidth), 1), cols)
    const row = Math.min(Math.max(Math.ceil((touchY - rect.top) / cellHeight), 1), rows)
    emit(`\x1b[<${up ? 64 : 65};${col};${row}M`.repeat(count))
    return
  }

  // Otherwise translate to cursor keys, honoring application cursor key mode.
  const app = term.modes.applicationCursorKeysMode
  emit((up ? (app ? '\x1bOA' : '\x1b[A') : app ? '\x1bOB' : '\x1b[B').repeat(count))
}

// Load the bundled Nerd Font at module import so it's usually ready by the
// time the user navigates to the terminal screen. xterm measures glyph
// width at construction time from whatever font is actually loaded — if we
// don't wait, it locks in Menlo metrics and columns misalign once the real
// font swaps in. The init effect awaits this promise before constructing
// XTerm. Catches failures silently so the terminal still comes up with the
// system monospace stack.
const fontReady: Promise<void> = (() => {
  if (typeof document === 'undefined' || typeof FontFace === 'undefined') {
    return Promise.resolve()
  }
  try {
    const face = new FontFace(PRIMARY_FONT, `url(${jetBrainsMonoRegular}) format('truetype')`)
    return face
      .load()
      .then((loaded) => {
        // FontFaceSet is Set-like per spec but TypeScript's lib.dom doesn't
        // model `.add()` on it. Cast through a minimal structural type.
        const fontSet = document.fonts as FontFaceSet & { add(font: FontFace): void }
        fontSet.add(loaded)
      })
      .catch(() => {
        /* fall back to system mono */
      })
  } catch {
    return Promise.resolve()
  }
})()

export type TerminalViewHandle = {
  /** Write raw data (can include ANSI escapes) into the terminal. */
  write: (chunk: string) => void
  /** Clear the terminal screen. */
  clear: () => void
  /** Force the xterm instance to a specific cols/rows. */
  resize: (cols: number, rows: number) => void
  /** Focus the terminal so the soft keyboard pops. */
  focus: () => void
  /** Blur the terminal's textarea to dismiss the soft keyboard. */
  blur: () => void
  /** Re-run FitAddon against the current host dims and push them upstream. */
  refit: () => void
}

type Props = {
  themeMode: 'light' | 'dark'
  terminalThemeId: TerminalThemeId
  onInput: (data: string) => Promise<void>
  onResize: (cols: number, rows: number) => Promise<void>
  onCopyRequest: (text: string) => Promise<void>
  onPasteRequest: () => Promise<void>
  onToolbarNotice: (message: string) => void
  /**
   * `ref` is intentionally part of Props rather than a `forwardRef` argument:
   * Expo DOM components auto-generate a wrapper on the native side that
   * reads `ref` as a first-class prop and installs a Proxy which forwards
   * method calls to `window._domRefProxy.<method>` via `injectJavaScript`.
   */
  ref?: React.Ref<TerminalViewHandle>
  dom?: import('expo/dom').DOMProps
}

export default function TerminalView({
  ref,
  themeMode,
  terminalThemeId,
  onInput,
  onResize,
  onCopyRequest,
  onPasteRequest,
  onToolbarNotice,
}: Props): React.ReactElement {
  const containerRef = useRef<HTMLDivElement>(null)
  const hostRef = useRef<HTMLDivElement>(null)
  const termRef = useRef<XTerm | null>(null)

  // Soft-keyboard toolbar state. The toolbar renders inside the WebView
  // (not as a React Native sibling) so taps on it can't make WKWebView
  // resign first responder — if they did, the iOS keyboard would snap shut
  // every time the user tried to press Ctrl/Esc/Tab. See `onMouseDown`
  // below: it preventDefault's to stop the button stealing focus from
  // xterm's internal textarea on the native-side pointer event.
  const [keyboardVisible, setKeyboardVisible] = useState(false)
  const [ctrlArmed, setCtrlArmed] = useState(false)
  const [altArmed, setAltArmed] = useState(false)

  // Mirror armed state into refs so the xterm `onData` handler — which is
  // registered once inside the init effect and runs for every keystroke —
  // can read the latest values without being re-registered on each toggle.
  const ctrlArmedRef = useRef(false)
  const altArmedRef = useRef(false)

  // Mirror the latest theme inputs into refs so the init effect can read
  // them without listing them in deps. Listing them would rebuild the whole
  // xterm instance (losing scrollback) on every theme change. The separate
  // live-update effect below handles theme swaps in place via
  // `term.options.theme = ...`.
  const themeModeRef = useRef(themeMode)
  themeModeRef.current = themeMode
  const terminalThemeIdRef = useRef(terminalThemeId)
  terminalThemeIdRef.current = terminalThemeId

  // onInput/onResize are already stable refs in the parent (see
  // terminal.tsx), but mirror them here too so we can keep the init effect's
  // deps empty without tripping react-hooks/exhaustive-deps.
  const onInputRef = useRef(onInput)
  onInputRef.current = onInput
  const onResizeRef = useRef(onResize)
  onResizeRef.current = onResize
  const onCopyRequestRef = useRef(onCopyRequest)
  onCopyRequestRef.current = onCopyRequest
  const onPasteRequestRef = useRef(onPasteRequest)
  onPasteRequestRef.current = onPasteRequest
  const onToolbarNoticeRef = useRef(onToolbarNotice)
  onToolbarNoticeRef.current = onToolbarNotice

  // `tryFit` is defined inside the init effect so it closes over the local
  // `fit`/`term`. Stash it in a ref so the imperative `refit()` method can
  // invoke it from outside the effect — used by terminal.tsx to re-fit on
  // tab switches when the visible host height (e.g. keyboard-adjusted) is
  // unchanged but the xterm was clobbered by a pty.resized replay event.
  const tryFitRef = useRef<(() => void) | null>(null)

  const blurTerminal = useCallback((): void => {
    try {
      termRef.current?.blur()
      termRef.current?.textarea?.blur()
    } catch {
      /* ignore — xterm might already be disposed */
    }
    try {
      if (typeof document !== 'undefined') {
        const active = document.activeElement as HTMLElement | null
        active?.blur?.()
      }
    } catch {
      /* ignore */
    }
  }, [])

  // Expo's DOM-component-aware variant of useImperativeHandle. Methods must
  // be serializable (JSON args, void return) because they cross the native
  // ↔ webview bridge via injectJavaScript. We type the handle more
  // specifically on the native side; the cast here adapts it to Expo's
  // generic `DOMImperativeFactory` index signature.
  useDOMImperativeHandle(
    (ref ?? null) as React.Ref<DOMImperativeFactory>,
    () => ({
      write: (chunk: unknown) => {
        if (typeof chunk !== 'string') return
        termRef.current?.write(chunk)
      },
      clear: () => {
        termRef.current?.clear()
      },
      resize: (cols: unknown, rows: unknown) => {
        if (typeof cols !== 'number' || typeof rows !== 'number') return
        termRef.current?.resize(cols, rows)
      },
      focus: () => {
        termRef.current?.focus()
      },
      blur: blurTerminal,
      refit: () => {
        tryFitRef.current?.()
      },
    }),
    [blurTerminal],
  )

  useEffect(() => {
    const host = hostRef.current
    if (!host) return

    let cancelled = false
    let cleanupFn: (() => void) | null = null

    const init = async (): Promise<void> => {
      // Wait for the Nerd Font to be loaded so xterm measures glyph width
      // against the real font, not the Menlo fallback.
      await fontReady
      if (cancelled) return
      if (!hostRef.current) return

      const initialPalette = resolveTerminalPalette(
        terminalThemeIdRef.current,
        themeModeRef.current,
      )

      const term = new XTerm({
        fontFamily: FONT_STACK,
        fontSize: 13,
        lineHeight: 1,
        convertEol: true,
        cursorBlink: true,
        allowProposedApi: true,
        screenReaderMode: true,
        theme: initialPalette,
      })

      const fit = new FitAddon()
      term.loadAddon(fit)
      term.open(host)
      termRef.current = term

      const a11y = host.querySelector<HTMLElement>('.xterm-accessibility')
      if (a11y) {
        a11y.style.pointerEvents = 'auto'
      }

      // Forward every keystroke to the native side. No local echo — the
      // remote PTY will echo back through the stream channel.
      //
      // The sticky Ctrl / Alt toolbar buttons intercept the next keystroke
      // here: when Ctrl is armed, we rewrite the typed letter into the
      // corresponding control byte (`code & 0x1f`, the standard Ctrl+char
      // mapping — works for a-z, A-Z, `@[\]^_` and space→NUL), then
      // auto-release. Alt prepends ESC to the typed sequence, matching
      // xterm's Meta behavior. Both release after a single keystroke so
      // the user can type normally again.
      term.onData((data) => {
        let out = data
        if (ctrlArmedRef.current && out.length === 1) {
          const code = out.charCodeAt(0)
          if (code >= 0x40 && code <= 0x7f) {
            out = String.fromCharCode(code & 0x1f)
          } else if (code === 0x20) {
            out = '\x00'
          }
          ctrlArmedRef.current = false
          setCtrlArmed(false)
        }
        if (altArmedRef.current) {
          out = '\x1b' + out
          altArmedRef.current = false
          setAltArmed(false)
        }
        void onInputRef.current(out)
      })

      // Tap / scroll / long-press gesture detector.
      //
      // Three distinct gestures on the terminal surface:
      //   tap   (< TAP_MAX_MS, < TAP_SLOP movement) → focus terminal, pop keyboard
      //   scroll (movement > TAP_SLOP before long-press threshold) → scroll content
      //   long-press (stationary > LONG_PRESS_MS) → native text selection via a11y tree
      //
      // Vertical finger movement during scroll drives the normal-buffer
      // scrollback via `term.scrollLines()`; on the alternate buffer (full-screen
      // TUIs with no scrollback) it's forwarded to the app as wheel/arrow input
      // via emitAltScrollInput(). Swiping DOWN reveals older content — same
      // direction as pull-to-scroll on native iOS lists.
      const TAP_SLOP = 8
      const TAP_MAX_MS = 500
      const LONG_PRESS_MS = 400

      let touchActive = false
      let touchStartX = 0
      let touchStartY = 0
      let touchStartTime = 0
      let touchLastY = 0
      let scrollAccumulator = 0
      let didScroll = false
      let isLongPress = false
      let longPressTimer: ReturnType<typeof setTimeout> | null = null

      const onTouchStart = (ev: TouchEvent): void => {
        if (ev.touches.length !== 1) {
          touchActive = false
          return
        }
        const t = ev.touches[0]
        touchActive = true
        touchStartX = t.clientX
        touchStartY = t.clientY
        touchStartTime = Date.now()
        touchLastY = t.clientY
        scrollAccumulator = 0
        didScroll = false
        isLongPress = false

        if (longPressTimer) clearTimeout(longPressTimer)
        longPressTimer = setTimeout(() => {
          longPressTimer = null
          if (!didScroll) {
            isLongPress = true
            term.textarea?.blur()
          }
        }, LONG_PRESS_MS)
      }

      const onTouchMove = (ev: TouchEvent): void => {
        if (!touchActive || ev.touches.length !== 1) return
        if (isLongPress) return

        const sel = document.getSelection()
        if (sel && !sel.isCollapsed) {
          isLongPress = true
          if (longPressTimer) {
            clearTimeout(longPressTimer)
            longPressTimer = null
          }
          return
        }

        const t = ev.touches[0]
        const dx = t.clientX - touchStartX
        const dy = t.clientY - touchStartY
        if (!didScroll && Math.hypot(dx, dy) > TAP_SLOP) {
          if (longPressTimer) {
            clearTimeout(longPressTimer)
            longPressTimer = null
          }
          didScroll = true
        }
        if (!didScroll) return

        ev.preventDefault()

        const moveDelta = t.clientY - touchLastY
        touchLastY = t.clientY
        scrollAccumulator += moveDelta

        const pixelsPerRow = term.rows > 0 ? host.clientHeight / term.rows : 0
        if (pixelsPerRow <= 0) return

        const lines = Math.trunc(scrollAccumulator / pixelsPerRow)
        if (lines !== 0) {
          // Alternate-screen TUIs have no scrollback — forward the swipe to the
          // app as its own scroll input; otherwise drive xterm's scrollback.
          if (term.buffer.active.type === 'alternate') {
            emitAltScrollInput(term, lines, t.clientX, t.clientY, host, (seq) =>
              onInputRef.current(seq),
            )
          } else {
            term.scrollLines(-lines)
          }
          scrollAccumulator -= lines * pixelsPerRow
        }
      }

      const onTouchEnd = (): void => {
        if (longPressTimer) {
          clearTimeout(longPressTimer)
          longPressTimer = null
        }
        if (!touchActive) return
        touchActive = false
        const elapsed = Date.now() - touchStartTime
        if (!didScroll && !isLongPress && elapsed <= TAP_MAX_MS) {
          const sel = document.getSelection()
          if (!sel || sel.isCollapsed) {
            term.focus()
          }
        }
      }

      const onTouchCancel = (): void => {
        if (longPressTimer) {
          clearTimeout(longPressTimer)
          longPressTimer = null
        }
        touchActive = false
        didScroll = false
        isLongPress = false
        scrollAccumulator = 0
      }

      // Selection-edge auto-scroll: when the user drags a selection
      // handle to the top/bottom edge of the terminal, scroll the xterm
      // buffer and programmatically restore the DOM selection on the
      // refreshed accessibility tree nodes via setBaseAndExtent().
      const EDGE_ZONE = 30
      const AUTO_SCROLL_MS = 150
      let autoScrollInterval: ReturnType<typeof setInterval> | null = null
      let isAutoScrolling = false

      const stopAutoScroll = (): void => {
        if (autoScrollInterval) {
          clearInterval(autoScrollInterval)
          autoScrollInterval = null
        }
      }

      const findRowIndex = (node: Node | null, tree: Element): number => {
        if (!node) return -1
        const rows = tree.children
        for (let i = 0; i < rows.length; i++) {
          if (rows[i] === node || rows[i].contains(node)) return i
        }
        return -1
      }

      const doAutoScrollStep = (direction: number): void => {
        const sel = document.getSelection()
        if (!sel || sel.isCollapsed || !sel.rangeCount) {
          stopAutoScroll()
          return
        }
        const a11yTree = host.querySelector('.xterm-accessibility-tree')
        if (!a11yTree) {
          stopAutoScroll()
          return
        }

        const anchorRowIdx = findRowIndex(sel.anchorNode, a11yTree)
        if (anchorRowIdx === -1) {
          stopAutoScroll()
          return
        }

        const anchorBufferRow = term.buffer.active.viewportY + anchorRowIdx
        const anchorOffset = sel.anchorOffset

        isAutoScrolling = true
        term.scrollLines(direction)

        setTimeout(() => {
          const newAnchorRow = anchorBufferRow - term.buffer.active.viewportY
          const rows = a11yTree.children
          if (newAnchorRow < 0 || newAnchorRow >= rows.length) {
            isAutoScrolling = false
            stopAutoScroll()
            return
          }
          const anchorEl = rows[newAnchorRow]
          const edgeIdx = direction > 0 ? rows.length - 1 : 0
          const edgeEl = rows[edgeIdx]
          if (!anchorEl?.firstChild || !edgeEl?.firstChild) {
            isAutoScrolling = false
            return
          }
          const newSel = document.getSelection()
          if (!newSel) {
            isAutoScrolling = false
            return
          }
          try {
            const edgeLen = edgeEl.textContent?.length ?? 0
            if (direction > 0) {
              newSel.setBaseAndExtent(anchorEl.firstChild, anchorOffset, edgeEl.firstChild, edgeLen)
            } else {
              newSel.setBaseAndExtent(edgeEl.firstChild, 0, anchorEl.firstChild, anchorOffset)
            }
          } catch {
            /* nodes may be detached */
          }
          isAutoScrolling = false
        }, 50)
      }

      const onSelectionChange = (): void => {
        if (isAutoScrolling) return
        const sel = document.getSelection()
        if (!sel || sel.isCollapsed) {
          stopAutoScroll()
          return
        }
        try {
          const range = sel.getRangeAt(0)
          const rects = range.getClientRects()
          if (rects.length === 0) {
            stopAutoScroll()
            return
          }
          const hostRect = host.getBoundingClientRect()
          const last = rects[rects.length - 1]
          const first = rects[0]
          if (last.bottom > hostRect.bottom - EDGE_ZONE) {
            if (!autoScrollInterval)
              autoScrollInterval = setInterval(() => doAutoScrollStep(1), AUTO_SCROLL_MS)
          } else if (first.top < hostRect.top + EDGE_ZONE) {
            if (!autoScrollInterval)
              autoScrollInterval = setInterval(() => doAutoScrollStep(-1), AUTO_SCROLL_MS)
          } else {
            stopAutoScroll()
          }
        } catch {
          stopAutoScroll()
        }
      }

      host.addEventListener('touchstart', onTouchStart, { passive: true })
      host.addEventListener('touchmove', onTouchMove, { passive: false })
      host.addEventListener('touchend', onTouchEnd, { passive: true })
      host.addEventListener('touchcancel', onTouchCancel, { passive: true })
      document.addEventListener('selectionchange', onSelectionChange)

      const tryFit = (): void => {
        try {
          fit.fit()
          void onResizeRef.current(term.cols, term.rows)
        } catch {
          // fit can throw briefly while the host element has zero size
        }
      }
      tryFitRef.current = tryFit

      // Debounced variant used by the host ResizeObserver and window
      // `resize` listener. The initial mount tryFit() is still sync because
      // no keyboard is animating at that point.
      //
      // On iOS the first terminal tap opens the keyboard and WKWebView
      // starts animating its visualViewport down. Our visualViewport.resize
      // handler writes the new height into host.style.height each frame,
      // which fires the ResizeObserver, which — without this debounce —
      // would call fit.fit() and term.resize() during the animation. That
      // DOM reflow makes WKWebView resign first responder and immediately
      // dismiss the keyboard again. Delaying the fit until the viewport
      // settles lets the keyboard animation complete first.
      let fitDebounce: ReturnType<typeof setTimeout> | null = null
      const scheduleFit = (): void => {
        if (fitDebounce !== null) clearTimeout(fitDebounce)
        fitDebounce = setTimeout(() => {
          fitDebounce = null
          tryFit()
        }, 150)
      }

      tryFit()

      const resizeObserver =
        typeof ResizeObserver !== 'undefined' ? new ResizeObserver(() => scheduleFit()) : null
      resizeObserver?.observe(host)
      window.addEventListener('resize', scheduleFit)

      document.body.style.margin = '0'
      document.body.style.padding = '0'
      document.body.style.backgroundColor = initialPalette.background

      // Size the outer container to the visible viewport (visualViewport)
      // rather than 100vh. When the iOS soft keyboard slides up, WKWebView
      // shrinks `visualViewport.height` to the area above the keyboard but
      // keeps `100vh` at the full WebView frame height — so `100vh` would
      // leave half the terminal hidden under the keyboard. visualViewport
      // .resize fires as the keyboard animates, we reflect it into the
      // container's height, ResizeObserver catches the resulting change on
      // `host` (flex child), tryFit recomputes cols/rows, and onResize
      // pushes the new size to the remote PTY.
      //
      // We target the container (not `host` directly) so the toolbar
      // sibling gets its share of the visible viewport via flex layout
      // without having to subtract its height manually on every resize.
      //
      // Crucially we do NOT resize the native View that hosts the WebView —
      // changing the WebView's outer frame while a textarea is focused
      // causes WKWebView to resign first responder and dismiss the keyboard
      // again right after it appears.
      const vv = typeof window !== 'undefined' ? window.visualViewport : null
      const container = containerRef.current
      const applyContainerHeight = (): void => {
        const target = container ?? host
        target.style.height = vv ? `${vv.height}px` : '100vh'
      }
      // Detect soft-keyboard state by comparing visualViewport height to
      // the full window height. WKWebView doesn't expose a direct keyboard
      // event, but whenever the keyboard slides up, visualViewport shrinks
      // by at least ~200px. The 100px threshold filters out incidental
      // diffs (e.g. transient safe-area animations on orientation change).
      const updateKeyboardVisible = (): void => {
        const kbUp = vv ? window.innerHeight - vv.height > 100 : false
        setKeyboardVisible(kbUp)
      }
      applyContainerHeight()
      updateKeyboardVisible()
      vv?.addEventListener('resize', applyContainerHeight)
      vv?.addEventListener('resize', updateKeyboardVisible)

      cleanupFn = () => {
        vv?.removeEventListener('resize', applyContainerHeight)
        vv?.removeEventListener('resize', updateKeyboardVisible)
        window.removeEventListener('resize', scheduleFit)
        host.removeEventListener('touchstart', onTouchStart)
        host.removeEventListener('touchmove', onTouchMove)
        host.removeEventListener('touchend', onTouchEnd)
        host.removeEventListener('touchcancel', onTouchCancel)
        document.removeEventListener('selectionchange', onSelectionChange)
        if (longPressTimer) clearTimeout(longPressTimer)
        stopAutoScroll()
        resizeObserver?.disconnect()
        if (fitDebounce !== null) {
          clearTimeout(fitDebounce)
          fitDebounce = null
        }
        tryFitRef.current = null
        term.dispose()
        termRef.current = null
      }

      // If the effect was already torn down while awaiting the font, run
      // cleanup now so we don't leak the XTerm instance we just created.
      if (cancelled) {
        cleanupFn()
        cleanupFn = null
      }
    }

    void init()

    return () => {
      cancelled = true
      cleanupFn?.()
    }
  }, [])

  // Live theme swap: xterm v5+ accepts an in-place theme assignment via
  // `term.options.theme = ...`, preserving scrollback. Deps are two
  // primitive strings (stable across the Expo DOM bridge), so this only
  // fires when the user actually picks a new preset or flips light/dark.
  useEffect(() => {
    const term = termRef.current
    if (!term) return
    const palette = resolveTerminalPalette(terminalThemeId, themeMode)
    term.options.theme = palette
    document.body.style.backgroundColor = palette.background
  }, [terminalThemeId, themeMode])

  const emit = useCallback((seq: string): void => {
    void onInputRef.current(seq)
  }, [])

  const pasteFromClipboard = useCallback((): void => {
    void onPasteRequestRef.current()
  }, [])

  const copySelection = useCallback((): void => {
    const selectedText = document.getSelection()?.toString() ?? ''
    if (!selectedText) {
      void onToolbarNoticeRef.current('No terminal selection to copy')
      return
    }
    void onCopyRequestRef.current(selectedText)
  }, [])

  const toggleCtrl = useCallback((): void => {
    const next = !ctrlArmedRef.current
    ctrlArmedRef.current = next
    setCtrlArmed(next)
  }, [])

  const toggleAlt = useCallback((): void => {
    const next = !altArmedRef.current
    altArmedRef.current = next
    setAltArmed(next)
  }, [])

  const palette = resolveTerminalPalette(terminalThemeId, themeMode)

  return (
    <div
      ref={containerRef}
      style={{
        display: 'flex',
        flexDirection: 'column',
        width: '100%',
        height: '100%',
      }}
    >
      <div style={{ flex: 1, minHeight: 0, overflow: 'hidden', padding: 4 }}>
        <div ref={hostRef} style={{ width: '100%', height: '100%' }} />
      </div>
      <div
        style={{
          display: keyboardVisible ? 'flex' : 'none',
          flexDirection: 'row',
          alignItems: 'center',
          gap: 6,
          padding: '6px 8px',
          overflowX: 'auto',
          background: palette.background,
          borderTop: `1px solid ${palette.foreground}33`,
          flexShrink: 0,
        }}
        // Any pointer-down that lands on the toolbar strip (between or
        // around buttons) must not steal focus from xterm's textarea,
        // otherwise the iOS soft keyboard dismisses on every tap gap.
        onMouseDown={(e): void => e.preventDefault()}
      >
        <ToolbarKey label="Hide" onPress={blurTerminal} palette={palette} />
        <ToolbarKey label="Copy" onPress={copySelection} palette={palette} />
        <ToolbarKey label="Paste" onPress={pasteFromClipboard} palette={palette} />
        <ToolbarKey label="Esc" onPress={(): void => emit('\x1b')} palette={palette} />
        <ToolbarKey label="Tab" onPress={(): void => emit('\t')} palette={palette} />
        <ToolbarKey label="Shift+Tab" onPress={(): void => emit('\x1b[Z')} palette={palette} />
        <ToolbarKey label="Ctrl" onPress={toggleCtrl} palette={palette} active={ctrlArmed} />
        <ToolbarKey label="Alt" onPress={toggleAlt} palette={palette} active={altArmed} />
        <ToolbarKey label="Left" onPress={(): void => emit('\x1b[D')} palette={palette} />
        <ToolbarKey label="Right" onPress={(): void => emit('\x1b[C')} palette={palette} />
        <ToolbarKey label="Up" onPress={(): void => emit('\x1b[A')} palette={palette} />
        <ToolbarKey label="Down" onPress={(): void => emit('\x1b[B')} palette={palette} />
        <ToolbarKey label="Home" onPress={(): void => emit('\x1b[H')} palette={palette} />
        <ToolbarKey label="End" onPress={(): void => emit('\x1b[F')} palette={palette} />
        <ToolbarKey label="Enter" onPress={(): void => emit('\r')} palette={palette} />
      </div>
    </div>
  )
}

// A single toolbar button. Rendered as a `<button>` so it gets native
// click semantics (iOS VoiceOver, etc.). The `onMouseDown` handler is the
// critical bit: browsers move focus to the clicked element on mousedown
// *before* click fires, which would resign xterm's textarea as first
// responder and dismiss the iOS keyboard. `preventDefault()` blocks that
// focus shift but leaves the click event intact, so `onPress` still runs.
function ToolbarKey({
  label,
  onPress,
  palette,
  active = false,
}: {
  label: string
  onPress: () => void
  palette: TerminalPalette
  active?: boolean
}): React.ReactElement {
  return (
    <button
      type="button"
      tabIndex={-1}
      onMouseDown={(e): void => e.preventDefault()}
      onClick={onPress}
      style={{
        minWidth: 44,
        height: 36,
        padding: '0 10px',
        fontFamily: FONT_STACK,
        fontSize: 14,
        fontWeight: active ? 600 : 400,
        color: active ? palette.background : palette.foreground,
        background: active ? palette.foreground : `${palette.foreground}1a`,
        border: `1px solid ${palette.foreground}44`,
        borderRadius: 6,
        cursor: 'pointer',
        flexShrink: 0,
        userSelect: 'none',
        WebkitUserSelect: 'none',
        WebkitTouchCallout: 'none',
        WebkitTapHighlightColor: 'transparent',
        touchAction: 'manipulation',
      }}
    >
      {label}
    </button>
  )
}
