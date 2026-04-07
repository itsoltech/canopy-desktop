import { match } from 'ts-pattern'

/** Platform-aware label for the native file manager action. */
export function fileManagerLabel(): string {
  return match(window.api.platform)
    .with('darwin', () => 'Reveal in Finder')
    .with('win32', () => 'Show in Explorer')
    .otherwise(() => 'Open in File Manager')
}
