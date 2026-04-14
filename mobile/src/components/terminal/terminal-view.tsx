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
      blur: () => {
        // xterm's own blur unfocuses its internal textarea. We also blur
        // whatever document.activeElement currently is as a belt-and-braces
        // measure: iOS WKWebView dismisses the soft keyboard when no
        // element inside the document holds focus.
        try {
          termRef.current?.blur()
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

      // Tap-vs-scroll gesture detector.
      //
      // - A brief, near-stationary touch focuses the terminal (and pops the
      //   soft keyboard). Anything further than TAP_SLOP pixels or longer
      //   than TAP_MAX_MS counts as a scroll instead and never focuses.
      // - Vertical finger movement is forwarded to xterm's scrollback via
      //   `term.scrollLines()`. We convert accumulated pixel deltas into
      //   whole rows using the current host height / term.rows ratio, so
      //   the scroll speed matches the visible row height regardless of
      //   devicePixelRatio. Swiping DOWN (finger moves toward bottom)
      //   reveals older content — same direction as pull-to-scroll on
      //   native iOS lists.
      const TAP_SLOP = 8
      const TAP_MAX_MS = 500

      let touchActive = false
      let touchStartX = 0
      let touchStartY = 0
      let touchStartTime = 0
      let touchLastY = 0
      let scrollAccumulator = 0
      let didScroll = false

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
      }

      const onTouchMove = (ev: TouchEvent): void => {
        if (!touchActive || ev.touches.length !== 1) return
        const t = ev.touches[0]
        const dx = t.clientX - touchStartX
        const dy = t.clientY - touchStartY
        if (!didScroll && Math.hypot(dx, dy) > TAP_SLOP) {
          didScroll = true
        }
        if (!didScroll) return

        // Block any default behaviour (rubber-band, text selection) now
        // that we're claiming this gesture for scrolling. touchmove is
        // registered with passive:false so preventDefault is honoured.
        ev.preventDefault()

        const moveDelta = t.clientY - touchLastY
        touchLastY = t.clientY
        scrollAccumulator += moveDelta

        const pixelsPerRow = term.rows > 0 ? host.clientHeight / term.rows : 0
        if (pixelsPerRow <= 0) return

        const lines = Math.trunc(scrollAccumulator / pixelsPerRow)
        if (lines !== 0) {
          // Finger down (positive dy) → scroll UP in the buffer (older
          // content). xterm's scrollLines uses the opposite sign.
          term.scrollLines(-lines)
          scrollAccumulator -= lines * pixelsPerRow
        }
      }

      const onTouchEnd = (): void => {
        if (!touchActive) return
        const elapsed = Date.now() - touchStartTime
        const wasTap = !didScroll && elapsed <= TAP_MAX_MS
        touchActive = false
        if (wasTap) {
          term.focus()
        }
      }

      const onTouchCancel = (): void => {
        touchActive = false
        didScroll = false
        scrollAccumulator = 0
      }

      host.addEventListener('touchstart', onTouchStart, { passive: true })
      host.addEventListener('touchmove', onTouchMove, { passive: false })
      host.addEventListener('touchend', onTouchEnd, { passive: true })
      host.addEventListener('touchcancel', onTouchCancel, { passive: true })

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
      <div ref={hostRef} style={{ flex: 1, minHeight: 0, overflow: 'hidden' }} />
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
        <ToolbarKey label="Esc" onPress={(): void => emit('\x1b')} palette={palette} />
        <ToolbarKey label="Tab" onPress={(): void => emit('\t')} palette={palette} />
        <ToolbarKey label="⇧Tab" onPress={(): void => emit('\x1b[Z')} palette={palette} />
        <ToolbarKey label="Ctrl" onPress={toggleCtrl} palette={palette} active={ctrlArmed} />
        <ToolbarKey label="Alt" onPress={toggleAlt} palette={palette} active={altArmed} />
        <ToolbarKey label="←" onPress={(): void => emit('\x1b[D')} palette={palette} />
        <ToolbarKey label="→" onPress={(): void => emit('\x1b[C')} palette={palette} />
        <ToolbarKey label="↑" onPress={(): void => emit('\x1b[A')} palette={palette} />
        <ToolbarKey label="↓" onPress={(): void => emit('\x1b[B')} palette={palette} />
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
