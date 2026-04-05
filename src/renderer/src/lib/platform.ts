/** Platform-aware label for the native file manager action. */
export function fileManagerLabel(): string {
  switch (window.api.platform) {
    case 'darwin':
      return 'Reveal in Finder'
    case 'win32':
      return 'Show in Explorer'
    default:
      return 'Open in File Manager'
  }
}
