import { match } from 'ts-pattern'

/**
 * Tagged error union for the remote-control feature in the main process.
 *
 * Distinct from `src/renderer-shared/remote/errors.ts` which is the renderer-side
 * error type — they don't share a vocabulary because the failure modes are different
 * (network bind, filesystem, certificate vs. peer connection state).
 */
export type RemoteServerError =
  | { _tag: 'AlreadyRunning' }
  | { _tag: 'NotRunning' }
  | { _tag: 'NoNetworkInterface' }
  | { _tag: 'PortBindFailed'; message: string }
  | { _tag: 'BundleNotFound'; path: string }
  | { _tag: 'TokenInvalid' }
  | { _tag: 'NoPendingPeer' }
  | { _tag: 'PeerLimitReached' }
  | { _tag: 'CertificateError'; message: string }

export function remoteServerErrorMessage(error: RemoteServerError): string {
  return match(error)
    .with({ _tag: 'AlreadyRunning' }, () => 'Remote control session is already running')
    .with({ _tag: 'NotRunning' }, () => 'Remote control session is not running')
    .with({ _tag: 'NoNetworkInterface' }, () => 'No usable network interface found on the LAN')
    .with({ _tag: 'PortBindFailed' }, (e) => `Failed to bind signaling server: ${e.message}`)
    .with({ _tag: 'BundleNotFound' }, (e) => `Remote client bundle not found at ${e.path}`)
    .with({ _tag: 'TokenInvalid' }, () => 'Invalid pairing token')
    .with({ _tag: 'NoPendingPeer' }, () => 'No peer is currently waiting to be accepted')
    .with({ _tag: 'PeerLimitReached' }, () => 'Another device is already paired')
    .with({ _tag: 'CertificateError' }, (e) => `Certificate error: ${e.message}`)
    .exhaustive()
}
