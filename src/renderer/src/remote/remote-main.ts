import { mount } from 'svelte'

import './assets/main.css'

import RemoteApp from './RemoteApp.svelte'

// Surface any unhandled errors at the top of the page so users who hit
// a bug on their phone don't have to connect Safari Web Inspector to
// figure out what's wrong. Without this the whole SPA can silently
// crash and the only visible symptom is a frozen "Connecting…" screen.
function showFatalError(label: string, err: unknown): void {
  const message = err instanceof Error ? `${err.message}\n${err.stack ?? ''}` : String(err)
  console.error(`[remote:${label}]`, err)
  const banner = document.createElement('div')
  banner.setAttribute(
    'style',
    'position:fixed;top:0;left:0;right:0;z-index:9999;' +
      'background:#2a1515;color:#ffcccc;font:12px/1.4 ui-monospace,monospace;' +
      'padding:12px 16px;white-space:pre-wrap;word-break:break-all;' +
      'border-bottom:1px solid #550000;max-height:50vh;overflow:auto',
  )
  banner.textContent = `[${label}] ${message}`
  document.body.appendChild(banner)
}

window.addEventListener('error', (e) => {
  showFatalError('uncaught', e.error ?? e.message)
})
window.addEventListener('unhandledrejection', (e) => {
  showFatalError('unhandled-promise', e.reason)
})

const app = mount(RemoteApp, {
  target: document.getElementById('app')!,
})

export default app
