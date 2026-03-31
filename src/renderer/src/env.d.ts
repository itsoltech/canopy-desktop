/// <reference types="svelte" />
/// <reference types="vite/client" />

declare namespace svelteHTML {
  interface IntrinsicElements {
    webview: import('svelte/elements').HTMLAttributes<HTMLElement> & {
      src?: string
      partition?: string
      webpreferences?: string
    }
  }
}
