'use dom'

import '@xterm/xterm/css/xterm.css'

import { FitAddon } from '@xterm/addon-fit'
import { Terminal as XTerm } from '@xterm/xterm'
import { type DOMImperativeFactory, useDOMImperativeHandle } from 'expo/dom'
import { useEffect, useRef } from 'react'

import { resolveTerminalPalette } from '../../constants/terminal-themes'
import type { TerminalThemeId } from '../../lib/storage/app-preferences-types'

export type TerminalViewHandle = {
  /** Write raw data (can include ANSI escapes) into the terminal. */
  write: (chunk: string) => void
  /** Clear the terminal screen. */
  clear: () => void
  /** Force the xterm instance to a specific cols/rows. */
  resize: (cols: number, rows: number) => void
  /** Focus the terminal so the soft keyboard pops. */
  focus: () => void
}

type Props = {
  themeMode: 'light' | 'dark'
  terminalThemeId: TerminalThemeId
  onInput: (data: string) => Promise<void>
  onResize: (cols: number, rows: number) => Promise<void>
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
}: Props): React.ReactElement {
  const hostRef = useRef<HTMLDivElement>(null)
  const termRef = useRef<XTerm | null>(null)

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
    }),
    [],
  )

  useEffect(() => {
    const host = hostRef.current
    if (!host) return

    const initialPalette = resolveTerminalPalette(terminalThemeIdRef.current, themeModeRef.current)

    const term = new XTerm({
      fontFamily: 'Menlo, Monaco, "SF Mono", monospace',
      fontSize: 13,
      lineHeight: 1,
      convertEol: true,
      cursorBlink: true,
      allowProposedApi: true,
      theme: initialPalette,
    })

    const fit = new FitAddon()
    term.loadAddon(fit)
    term.open(host)
    termRef.current = term

    // Forward every keystroke to the native side. No local echo — the
    // remote PTY will echo back through the stream channel.
    term.onData((data) => {
      void onInputRef.current(data)
    })

    const focusTerm = (): void => term.focus()
    host.addEventListener('pointerdown', focusTerm)
    host.addEventListener('touchstart', focusTerm, { passive: true })

    const tryFit = (): void => {
      try {
        fit.fit()
        void onResizeRef.current(term.cols, term.rows)
      } catch {
        // fit can throw briefly while the host element has zero size
      }
    }

    tryFit()

    const resizeObserver =
      typeof ResizeObserver !== 'undefined' ? new ResizeObserver(() => tryFit()) : null
    resizeObserver?.observe(host)
    window.addEventListener('resize', tryFit)

    document.body.style.margin = '0'
    document.body.style.padding = '0'
    document.body.style.backgroundColor = initialPalette.background

    // Size the xterm host to the visible viewport (visualViewport) rather
    // than 100vh. When the iOS soft keyboard slides up, WKWebView shrinks
    // `visualViewport.height` to the area above the keyboard but keeps
    // `100vh` at the full WebView frame height — so `100vh` would leave
    // half the terminal hidden under the keyboard. visualViewport.resize
    // fires as the keyboard animates, we reflect it into host.style.height,
    // ResizeObserver catches the change, tryFit recomputes cols/rows, and
    // onResize pushes the new size to the remote PTY.
    //
    // Crucially we do NOT resize the native View that hosts the WebView —
    // changing the WebView's outer frame while a textarea is focused causes
    // WKWebView to resign first responder and dismiss the keyboard again
    // right after it appears.
    const vv = typeof window !== 'undefined' ? window.visualViewport : null
    const applyHostHeight = (): void => {
      host.style.height = vv ? `${vv.height}px` : '100vh'
    }
    applyHostHeight()
    vv?.addEventListener('resize', applyHostHeight)

    return () => {
      vv?.removeEventListener('resize', applyHostHeight)
      window.removeEventListener('resize', tryFit)
      host.removeEventListener('pointerdown', focusTerm)
      host.removeEventListener('touchstart', focusTerm)
      resizeObserver?.disconnect()
      term.dispose()
      termRef.current = null
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

  return <div ref={hostRef} style={{ width: '100%', height: '100%' }} />
}
