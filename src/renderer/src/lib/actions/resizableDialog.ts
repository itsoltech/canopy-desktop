/**
 * For dialogs with CSS `resize`: keeps the compact default max-height until the user actually
 * grabs the resize handle (the browser then writes inline width/height), at which point the
 * size caps are lifted so the dialog can grow to most of the viewport.
 */
export function unlockSizeOnResize(node: HTMLElement): { destroy(): void } {
  const observer = new MutationObserver(() => {
    if (node.style.height || node.style.width) {
      node.style.maxHeight = '88vh'
      node.style.maxWidth = '94vw'
      observer.disconnect()
    }
  })
  observer.observe(node, { attributes: true, attributeFilter: ['style'] })
  return { destroy: () => observer.disconnect() }
}
