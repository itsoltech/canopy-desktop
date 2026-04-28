<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { Terminal } from '@xterm/xterm'
  // xterm's own CSS ships the hide rules for `.xterm-char-measure-element`
  // and `.xterm-helper-textarea` — without importing it here, those helper
  // elements render as visible layout on the remote client, leaking into
  // the mobile viewport. Desktop already imports this via TerminalInstance,
  // but the remote bundle has a separate entry point and needs its own.
  import '@xterm/xterm/css/xterm.css'
  import type { RemoteApi } from '../lib/api'

  let {
    sessionId,
    api,
    onClose,
    keyboardOpen = false,
  }: {
    sessionId: string
    api: RemoteApi
    onClose?: () => void
    /**
     * Is the mobile soft keyboard currently open? When it flips to true
     * we scroll the terminal to the bottom so the last line (where the
     * user is typing) stays visible above the keyboard. Passed in from
     * `RemoteApp.svelte` which owns the shared `viewportTracker`.
     */
    keyboardOpen?: boolean
  } = $props()

  let scrollEl: HTMLDivElement | undefined = $state()
  let containerEl: HTMLDivElement | undefined = $state()
  let term: Terminal | null = null
  let unsubData: (() => void) | null = null
  let unsubClosed: (() => void) | null = null
  let unsubResized: (() => void) | null = null

  // ===== Smart auto-follow cursor state =====
  //
  // The outer `.terminal-scroll` element scrolls horizontally across the
  // host PTY's (possibly wide) canvas. When the CLI on the host prints
  // output the cursor moves — we want the viewer's scroll to follow the
  // cursor so the user always sees the latest prompt column. BUT: if the
  // user manually pans to re-read something earlier on the line, we stop
  // following so we don't fight their finger. Resuming happens when they
  // scroll back to (or past) the cursor column.
  //
  // This is the "tail -f" pattern adapted to horizontal scrolling.

  /** Scroll-mode flip: `true` means the user is manually scrolled away
   *  from the cursor and we should NOT auto-follow. Resumed to `false`
   *  when the user scrolls the cursor back into view. */
  let stickyManualScroll = false
  /** Last scrollLeft value we set programmatically. Used to distinguish
   *  our own scroll writes from a user-driven scroll inside the scroll
   *  event handler — if they match (within 1px), it was us. */
  let lastProgrammaticScrollLeft = 0
  /** Debounce flag for `requestAnimationFrame` so we don't schedule
   *  multiple auto-follow ticks for the same paint. */
  let rafScheduled = false
  /** Pixel width of one monospace character at the current font size.
   *  Populated after the first render (we can't measure before xterm
   *  paints a cell). Fallback 8px is close enough for initial seeding. */
  let charWidth = 8

  /** Read one cell's width from the rendered xterm. We MUST measure from
   *  `.xterm-screen` (not the outer `.terminal-wrap`): xterm sizes
   *  `.xterm-screen` to exactly `cols * cellWidth`, whereas our wrap has
   *  `min-width: 100%` so its `scrollWidth` is clamped to the viewer's
   *  viewport when the PTY is narrower than the screen. Measuring the
   *  wrap would yield `375 / 50 = 7.5` even though the real cell width
   *  is ~7.0 — and that drift would compound on every re-fit, shaving a
   *  column off the PTY on every tap. (Feedback loop that shrinks the
   *  PTY to nothing after ~20 taps.) */
  function measureCharWidth(): number {
    if (!containerEl || !term) return charWidth
    const screen = containerEl.querySelector<HTMLElement>('.xterm-screen')
    if (screen) {
      const rect = screen.getBoundingClientRect()
      if (rect.width > 0) return rect.width / Math.max(term.cols, 1)
    }
    return charWidth
  }

  /** Similarly stable row height — `.xterm-screen` height is exactly
   *  `rows * cellHeight`, so we don't have to rely on
   *  `fontSize * lineHeight` which depends on DPR rounding. */
  function measureRowHeight(): number {
    if (!containerEl || !term) return (term?.options.fontSize ?? 13) * 1.3
    const screen = containerEl.querySelector<HTMLElement>('.xterm-screen')
    if (screen) {
      const rect = screen.getBoundingClientRect()
      if (rect.height > 0) return rect.height / Math.max(term.rows, 1)
    }
    return (term?.options.fontSize ?? 13) * 1.3
  }

  /** Snap the scroll container so the terminal cursor sits at ~30% from
   *  the left edge of the viewport. That leaves 70% of width visible to
   *  the right of the cursor for incoming output, but also shows a bit
   *  of context to the left so you can see the start of the current
   *  prompt line. */
  function doAutoFollow(): void {
    if (!scrollEl || !term) return
    const cursorX = term.buffer.active.cursorX
    const cursorPx = cursorX * charWidth
    const maxScroll = Math.max(0, scrollEl.scrollWidth - scrollEl.clientWidth)
    const desired = Math.max(0, Math.min(cursorPx - scrollEl.clientWidth * 0.3, maxScroll))
    lastProgrammaticScrollLeft = desired
    scrollEl.scrollLeft = desired
  }

  /** Queue an auto-follow tick on the next animation frame. No-ops when
   *  already queued or when the user is in manual-scroll mode. */
  function scheduleAutoFollow(): void {
    if (rafScheduled || stickyManualScroll) return
    rafScheduled = true
    requestAnimationFrame(() => {
      rafScheduled = false
      doAutoFollow()
    })
  }

  /** User scroll handler: flips into manual mode if the scroll moved
   *  away from the cursor, or back into auto mode if it was dragged
   *  back. A 2-char tolerance keeps fat-finger swipes from bouncing
   *  between modes rapidly. */
  function onScrollEvent(): void {
    if (!scrollEl || !term) return
    // Ignore the scroll event we ourselves just fired
    if (Math.abs(scrollEl.scrollLeft - lastProgrammaticScrollLeft) < 1) return
    const cursorPx = term.buffer.active.cursorX * charWidth
    const viewRight = scrollEl.scrollLeft + scrollEl.clientWidth
    const tolerance = charWidth * 2
    stickyManualScroll = viewRight < cursorPx - tolerance
  }

  /**
   * Pick an xterm font size based on the viewport width. Narrower phones get
   * a smaller glyph so more of a 120-180 col host PTY fits on screen without
   * forcing the user to pan horizontally across a kilometre of scroll.
   *
   * Desktop (≥ 768px) keeps the original 13px — at that width the viewport
   * normally holds the whole PTY, so shrinking the font would just hurt
   * readability without saving any scroll distance.
   */
  function resolveMobileFontSize(): { fontSize: number; lineHeight: number } {
    const w = typeof window !== 'undefined' ? window.innerWidth : 768
    if (w >= 768) return { fontSize: 13, lineHeight: 1.3 }
    if (w >= 480) return { fontSize: 11, lineHeight: 1.25 }
    if (w >= 360) return { fontSize: 10, lineHeight: 1.2 }
    return { fontSize: 9, lineHeight: 1.15 }
  }

  /**
   * Build the xterm `theme` object from the active CSS custom properties
   * so the terminal colors track whatever theme the remote client SPA
   * is rendering in. Fallbacks kick in when a variable isn't defined
   * (first paint before tokens load, or if the remote client bundle
   * hasn't picked up the shared theme system yet).
   */
  function resolveXtermTheme(): {
    background: string
    foreground: string
    cursor: string
  } {
    if (typeof window === 'undefined') {
      return {
        background: 'var(--color-bg)',
        foreground: 'oklch(0.907 0 0)',
        cursor: 'var(--color-accent)',
      }
    }
    const style = getComputedStyle(document.documentElement)
    const read = (name: string, fallback: string): string => {
      const v = style.getPropertyValue(name).trim()
      return v.length > 0 ? v : fallback
    }
    return {
      background: read('--c-bg', 'var(--color-bg)'),
      foreground: read('--c-text', 'oklch(0.907 0 0)'),
      cursor: read('--c-accent-text', 'var(--color-accent)'),
    }
  }

  /**
   * Compute the cols/rows that fit the peer's scroll container. Used to
   * request a host-side PTY resize via `api.pty.resize` so the host PTY
   * matches the peer's viewport — otherwise a phone is stuck scrolling
   * across 180 cols of a desktop-wide terminal.
   *
   * Returns `null` when the container hasn't been laid out yet (first
   * paint), when charWidth couldn't be measured, or when the computed
   * dimensions are obviously garbage (< 10 cols / < 3 rows). Callers
   * should treat `null` as "try again on the next frame".
   *
   * Note: no artificial column buffer (`-2`) — earlier versions had one,
   * but combined with a drifting `measureCharWidth` result it caused the
   * PTY to shrink by ~2 cols on every single tap. With accurate cell
   * dimensions measured from `.xterm-screen`, the naive floor gives a
   * stable fixed point.
   */
  function computePeerFit(): { cols: number; rows: number } | null {
    if (!scrollEl || !term || !containerEl) return null
    // Account for `.terminal-wrap`'s 8px padding on each side — the xterm
    // canvas sits inside that padding, so subtract it from the available
    // width/height before dividing by cell dimensions.
    const wrapStyle = getComputedStyle(containerEl)
    const padX =
      parseFloat(wrapStyle.paddingLeft || '0') + parseFloat(wrapStyle.paddingRight || '0')
    const padY =
      parseFloat(wrapStyle.paddingTop || '0') + parseFloat(wrapStyle.paddingBottom || '0')
    const width = scrollEl.clientWidth - padX
    const height = scrollEl.clientHeight - padY
    if (width <= 0 || height <= 0) return null
    const cw = charWidth > 1 ? charWidth : 8
    const rh = measureRowHeight()
    const cols = Math.max(10, Math.floor(width / cw))
    const rows = Math.max(3, Math.floor(height / rh))
    return { cols, rows }
  }

  /**
   * Tell the host to resize its PTY to match our viewport. Debounced via
   * a flag because a burst of resize events (e.g. rotation + keyboard
   * open) would otherwise fire multiple RPC calls in flight simultaneously
   * and the last one to arrive (not necessarily the latest state) would
   * win on the host side.
   */
  let resizeInFlight = false
  async function requestPeerFit(): Promise<void> {
    if (resizeInFlight) return
    const fit = computePeerFit()
    if (!fit) return
    // Skip if we're already at the requested dimensions — no need for a
    // round trip just to reaffirm the same cols/rows.
    if (term && fit.cols === term.cols && fit.rows === term.rows) return
    resizeInFlight = true
    try {
      await api.pty.resize(sessionId, fit.cols, fit.rows)
    } catch (e) {
      console.warn('[remote-terminal] pty.resize failed:', e)
    } finally {
      resizeInFlight = false
    }
  }

  onMount(async () => {
    if (!containerEl) return

    // Fetch the host PTY's current dimensions BEFORE creating the xterm.
    // Without this, our local xterm defaults to 80×24 and wraps any host
    // output that used a wider window — shell prompts, Claude/Gemini CLI
    // output, tmux, htop, etc. all rely on cursor positioning escape
    // sequences tuned for the host's cols. A narrower viewer would see
    // corrupted alignment and double-wrapped lines.
    //
    // We pull the host's dimensions and build our xterm at the *exact*
    // same cols/rows. The xterm canvas then renders at its natural width
    // (cols × character width) and our container scrolls horizontally if
    // the viewer's viewport is narrower (common on phones). Alignment
    // stays correct, the user just pans sideways to read long lines.
    let cols = 80
    let rows = 24
    try {
      const dims = await api.pty.getDimensions(sessionId)
      if (dims && dims.cols > 0 && dims.rows > 0) {
        cols = dims.cols
        rows = dims.rows
      }
    } catch (e) {
      console.warn('[remote-terminal] getDimensions failed, using fallback:', e)
    }

    const { fontSize, lineHeight } = resolveMobileFontSize()

    term = new Terminal({
      cols,
      rows,
      fontSize,
      lineHeight,
      cursorBlink: true,
      scrollback: 5000,
      // Disable line wrap at the xterm level — the PTY already lays out
      // content for `cols` characters and any additional wrap here would
      // just undo that layout. If the host writes a 120-char line and the
      // viewer's xterm is also 120 cols, no wrap is needed.
      // (xterm.js doesn't have an explicit "no wrap" option; keeping cols
      // in sync with the host is the right fix.)
      theme: resolveXtermTheme(),
    })
    term.open(containerEl)

    // Measure the real character width once xterm has painted its first
    // frame — before that, `scrollWidth` is 0 and our auto-follow math
    // would target column 0. `onRender` fires on every frame; we only
    // need the first, so we latch it.
    let charWidthLatched = false
    term.onRender(() => {
      if (charWidthLatched) return
      charWidthLatched = true
      charWidth = measureCharWidth()
      // First auto-follow after paint so the initial prompt (usually at
      // col 0 for a fresh shell, but can be further right after a saved
      // tmux session) is positioned correctly.
      scheduleAutoFollow()
      // Now that we can measure characters, request the host PTY resize
      // to match our viewport. Without this, a phone-sized peer gets
      // 180+ cols of scroll from the host's desktop layout.
      void requestPeerFit()
    })

    // Scroll event handler: detect manual user panning so we know when to
    // stop / resume auto-follow. Listener lives on the outer scroll el.
    scrollEl?.addEventListener('scroll', onScrollEvent, { passive: true })

    // Tap-to-refit: any pointer interaction with the terminal re-runs the
    // peer fit request. That's how we express "whoever just touched this
    // PTY wins" — the mobile user tapping back into the terminal re-asserts
    // the mobile dimensions, and the desktop xterm's FitAddon (which runs
    // on focus and ResizeObserver) does the same thing in the other
    // direction. We use `pointerdown` instead of `click` so the resize
    // request goes out before xterm consumes the click to focus its
    // hidden textarea.
    //
    // IMPORTANT: skip the refit while the on-screen keyboard is already
    // open. In that state `scrollEl.clientHeight` is the SHRUNK viewport
    // (visual-viewport minus keyboard), and measuring it would burn the
    // keyboard-adjusted dimensions into the PTY. When the user then
    // dismisses the keyboard, the PTY would be stuck at the small size
    // while the container grows back — you'd see the terminal rendered
    // in just the top portion of an otherwise-empty scroll area. The
    // keyboard-close branch below handles re-fitting for the new
    // (larger) size once the keyboard is actually gone.
    containerEl.addEventListener('pointerdown', () => {
      if (keyboardOpen) return
      void requestPeerFit()
    })

    // Input: peer keyboard → PTY write. The xterm itself handles cursor
    // echo / line editing; we just proxy bytes to the host.
    term.onData((data) => {
      api.pty.write(sessionId, data).catch((e) => {
        console.warn('[remote-terminal] write failed:', e)
      })
    })

    // Subscribe to PTY output. After each chunk we queue an auto-follow
    // tick — by the time the RAF fires, xterm has parsed the chunk and
    // `term.buffer.active.cursorX` reflects the new cursor position.
    unsubData = api.subscribe<string>(`pty.data.${sessionId}`, (chunk) => {
      term?.write(chunk)
      scheduleAutoFollow()
    })

    unsubClosed = api.subscribe<null>(`pty.closed.${sessionId}`, () => {
      term?.write('\r\n\x1b[90m[session closed]\x1b[0m\r\n')
      onClose?.()
    })

    // Resize relay: when the host PTY changes dimensions (e.g. user
    // resizes the desktop Canopy window, a split pane fit addon re-runs),
    // the host renderer broadcasts a `pty.resized.<sessionId>` event with
    // the new cols/rows. We call `term.resize()` to match, then requeue
    // an auto-follow so the cursor stays visible after the rescale. Any
    // cached `charWidth` from before the resize is now wrong — measure
    // it again on the next frame so auto-follow math uses the new cell
    // pitch (xterm may change cell size if fontSize changes with DPR).
    unsubResized = api.subscribe<{ cols: number; rows: number }>(
      `pty.resized.${sessionId}`,
      (dims) => {
        if (!term) return
        try {
          term.resize(dims.cols, dims.rows)
          requestAnimationFrame(() => {
            charWidth = measureCharWidth()
            scheduleAutoFollow()
          })
        } catch (e) {
          console.warn('[remote-terminal] resize failed:', e)
        }
      },
    )

    // Tell host to start forwarding
    try {
      await api.pty.subscribe(sessionId)
    } catch (e) {
      term?.write(`\r\n\x1b[31m[failed to subscribe: ${e}]\x1b[0m\r\n`)
    }
  })

  // Keyboard state transitions — we need to react to BOTH open and close:
  //
  //   open  → scroll the terminal to the bottom so the cursor / last
  //           prompt line stays visible above the keyboard
  //   close → refit the PTY to the new (larger) viewport, because any
  //           tap on the terminal while the keyboard was up was skipped
  //           by the pointerdown handler. Without this, the container
  //           grows back but the PTY stays at the small keyboard-era
  //           size, leaving the terminal rendered in just the top slice
  //           of an otherwise-empty scroll region.
  //
  // We track the previous value in a plain local (NOT `$state`) so
  // assigning to it inside the effect doesn't re-trigger reactivity.
  // The effect re-runs when `keyboardOpen` (a reactive prop) changes.
  let prevKeyboardOpen = false
  $effect(() => {
    const isOpen = keyboardOpen
    if (isOpen && !prevKeyboardOpen) {
      // Keyboard just opened — scroll to bottom.
      if (term && scrollEl) {
        requestAnimationFrame(() => {
          term?.scrollToBottom()
          if (scrollEl) scrollEl.scrollTop = scrollEl.scrollHeight
          scheduleAutoFollow()
        })
      }
    } else if (!isOpen && prevKeyboardOpen) {
      // Keyboard just closed — the scroll container has grown back to
      // full viewport height. Refit the PTY so its rows match the new
      // space instead of being stuck at the shrunken dimensions from
      // when we last measured.
      requestAnimationFrame(() => {
        void requestPeerFit()
      })
    }
    prevKeyboardOpen = isOpen
  })

  onDestroy(() => {
    unsubData?.()
    unsubClosed?.()
    unsubResized?.()
    scrollEl?.removeEventListener('scroll', onScrollEvent)
    api.pty.unsubscribe(sessionId).catch(() => {})
    term?.dispose()
    term = null
  })
</script>

<div
  class="flex-1 min-h-0 w-full overflow-auto rounded-lg border border-border bg-bg [overscroll-behavior:contain] [touch-action:pan-x_pan-y]"
  bind:this={scrollEl}
>
  <div class="terminal-wrap w-fit min-w-full p-2" bind:this={containerEl}></div>
</div>

<!-- Global xterm overrides: neutralize xterm's internal layout/scrollbars so our outer scroll wins. -->
<style>
  .terminal-wrap :global(.xterm) {
    width: fit-content;
    height: auto;
  }

  .terminal-wrap :global(.xterm-viewport) {
    overflow: hidden !important;
    width: auto !important;
    height: auto !important;
    position: static !important;
  }

  .terminal-wrap :global(.xterm-screen) {
    width: fit-content;
  }
</style>
