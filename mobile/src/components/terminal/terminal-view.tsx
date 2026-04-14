'use dom'

import '@xterm/xterm/css/xterm.css'

import { FitAddon } from '@xterm/addon-fit'
import { Terminal as XTerm } from '@xterm/xterm'
import { type DOMImperativeFactory, useDOMImperativeHandle } from 'expo/dom'
import { useEffect, useRef } from 'react'

import jetBrainsMonoRegular from '../../../assets/fonts/JetBrainsMonoNerdFontMono-Regular.ttf'
import { resolveTerminalPalette } from '../../constants/terminal-themes'
import type { TerminalThemeId } from '../../lib/storage/app-preferences-types'

const PRIMARY_FONT = 'JetBrainsMonoNerdFontMono'
const FONT_STACK = `"${PRIMARY_FONT}", Menlo, Monaco, "SF Mono", monospace`

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
  /** Re-run FitAddon against the current host dims and push them upstream. */
  refit: () => void
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

  // `tryFit` is defined inside the init effect so it closes over the local
  // `fit`/`term`. Stash it in a ref so the imperative `refit()` method can
  // invoke it from outside the effect — used by terminal.tsx to re-fit on
  // tab switches when the visible host height (e.g. keyboard-adjusted) is
  // unchanged but the xterm was clobbered by a pty.resized replay event.
  const tryFitRef = useRef<(() => void) | null>(null)

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
      refit: () => {
        tryFitRef.current?.()
      },
    }),
    [],
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
      // changing the WebView's outer frame while a textarea is focused
      // causes WKWebView to resign first responder and dismiss the keyboard
      // again right after it appears.
      const vv = typeof window !== 'undefined' ? window.visualViewport : null
      const applyHostHeight = (): void => {
        host.style.height = vv ? `${vv.height}px` : '100vh'
      }
      applyHostHeight()
      vv?.addEventListener('resize', applyHostHeight)

      cleanupFn = () => {
        vv?.removeEventListener('resize', applyHostHeight)
        window.removeEventListener('resize', scheduleFit)
        host.removeEventListener('pointerdown', focusTerm)
        host.removeEventListener('touchstart', focusTerm)
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

  return <div ref={hostRef} style={{ width: '100%', height: '100%' }} />
}
