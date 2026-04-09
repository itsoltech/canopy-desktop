/**
 * Visual-viewport-based keyboard tracker for the remote client shell.
 *
 * ## Why this exists
 *
 * On mobile (especially iOS Safari) the soft keyboard does NOT shrink the
 * layout viewport — `window.innerHeight` stays constant and `100vh` / even
 * `100dvh` stay at the full screen height. The keyboard just draws on top
 * of the page, so our xterm-based preview (which fills the shell) ends up
 * half-obscured by the keyboard when the user taps into the terminal to
 * type a prompt to Claude or a shell.
 *
 * `window.visualViewport` tells us the VISUAL viewport — the subregion of
 * the layout viewport that's actually on screen. When the keyboard opens,
 * `visualViewport.height` drops by the keyboard's height. Subtracting it
 * from `window.innerHeight` gives the keyboard's apparent height.
 *
 * ## Usage
 *
 * ```ts
 * const viewport = createViewportTracker()
 * // reactive:
 * $derived(viewport.isKeyboardOpen ? 'shrink me' : 'normal')
 * // cleanup on unmount:
 * onDestroy(() => viewport.dispose())
 * ```
 *
 * The returned object exposes `keyboardHeight` and `isKeyboardOpen` as
 * getter properties that read from a `$state` rune under the hood, so they
 * participate in Svelte 5's reactive graph the same way a plain reactive
 * state would.
 *
 * ## Threshold
 *
 * iOS Safari also fires `visualViewport` resize events when the browser
 * chrome (address bar) collapses on scroll, which shows up as a ~50px
 * height diff. We ignore anything below 100px so those don't get
 * misidentified as keyboard openings — real keyboards are always much
 * taller than that.
 */
export interface ViewportTracker {
  readonly keyboardHeight: number
  readonly isKeyboardOpen: boolean
  /**
   * Current visual viewport height in CSS pixels. This is the source of
   * truth for sizing the shell — equal to `window.innerHeight` on desktop
   * and when no keyboard is open, and equal to `visualViewport.height`
   * (which excludes the on-screen keyboard) when the keyboard is open.
   * Falls back to 0 on SSR / non-browser environments.
   */
  readonly visualHeight: number
  dispose(): void
}

/**
 * Height difference (in CSS pixels) below which we treat a visualViewport
 * resize as "just the browser chrome moving" rather than a keyboard opening.
 * Real mobile keyboards on iOS/Android are 250-350px tall, so 100 is a safe
 * floor that excludes address-bar jitter.
 */
const KEYBOARD_HEIGHT_THRESHOLD = 100

export function createViewportTracker(): ViewportTracker {
  let keyboardHeight = $state(0)
  let visualHeight = $state(typeof window !== 'undefined' ? window.innerHeight : 0)

  // SSR / non-browser / desktop browsers without visualViewport (older
  // Safari, some embedded webviews) get a no-op tracker. Callers can use
  // the same reactive API without feature-detecting themselves.
  if (typeof window === 'undefined' || !window.visualViewport) {
    return {
      get keyboardHeight() {
        return 0
      },
      get isKeyboardOpen() {
        return false
      },
      get visualHeight() {
        return visualHeight
      },
      dispose() {
        // nothing to tear down
      },
    }
  }

  const vv = window.visualViewport

  const update = (): void => {
    // `window.innerHeight` is the layout viewport height — unchanged when
    // the keyboard opens. `vv.height` is the visible portion. We expose
    // BOTH the diff (keyboardHeight) and the raw visual height so the
    // shell can be sized directly from `visualHeight` without relying on
    // `calc(100dvh - ...)` — iOS has historically been inconsistent about
    // whether `100dvh` excludes the keyboard, and setting the height in
    // pixels sidesteps that ambiguity entirely.
    visualHeight = vv.height
    const diff = window.innerHeight - vv.height
    keyboardHeight = diff > KEYBOARD_HEIGHT_THRESHOLD ? diff : 0

    // ALSO publish as a global CSS variable on `document.documentElement`.
    // This is what actually drives `#app` height in the remote client's
    // `main.css` — `#app` is a sibling of our Svelte-mounted `.shell` in
    // the DOM tree and can't read a CSS variable we set as an inline
    // style on `.shell`. Writing it at the document root makes it
    // visible to every element in the page, including the hosting
    // layout wrapper that's OUTSIDE the Svelte component tree.
    document.documentElement.style.setProperty('--shell-height', `${vv.height}px`)
  }

  // `resize` fires on keyboard open/close and on orientation change.
  // `scroll` fires when the user pans the visual viewport (e.g. zoom +
  // drag), and we want to recompute then too because `vv.height` can
  // change as the keyboard animates in.
  vv.addEventListener('resize', update)
  vv.addEventListener('scroll', update)
  // Also listen to window resize so orientation changes and
  // window-restore-from-inactive events update `visualHeight` even on
  // browsers where visualViewport doesn't fire `resize` for those cases.
  window.addEventListener('resize', update)
  update()

  return {
    get keyboardHeight() {
      return keyboardHeight
    },
    get isKeyboardOpen() {
      return keyboardHeight > 0
    },
    get visualHeight() {
      return visualHeight
    },
    dispose() {
      vv.removeEventListener('resize', update)
      vv.removeEventListener('scroll', update)
      window.removeEventListener('resize', update)
      // Clean up the CSS var so a subsequent mount starts fresh (e.g.
      // navigation back to the same route without a full page reload).
      document.documentElement.style.removeProperty('--shell-height')
    },
  }
}
