import { match } from 'ts-pattern'

/**
 * Tagged error union for the remote-control feature, shared between
 * host renderer and remote peer. Each `_tag` corresponds to a distinct
 * failure mode and is exhaustively handled by `remoteErrorMessage`.
 */
export type RemoteError =
  | { _tag: 'NotEnabled' }
  | { _tag: 'AlreadyRunning' }
  | { _tag: 'PortBindFailed'; port: number; message: string }
  | { _tag: 'TokenInvalid' }
  | { _tag: 'PeerRejected' }
  | { _tag: 'ChannelDisconnected' }
  | { _tag: 'TimedOut' }
  | { _tag: 'CertificateError'; message: string }
  | { _tag: 'BundleNotFound'; path: string }
  | { _tag: 'NoNetworkInterface' }

export function remoteErrorMessage(e: RemoteError): string {
  return match(e)
    .with({ _tag: 'NotEnabled' }, () => 'Remote control is not enabled in settings')
    .with({ _tag: 'AlreadyRunning' }, () => 'A remote control session is already running')
    .with({ _tag: 'PortBindFailed' }, (x) => `Failed to bind on port ${x.port}: ${x.message}`)
    .with({ _tag: 'TokenInvalid' }, () => 'Invalid pairing token')
    .with({ _tag: 'PeerRejected' }, () => 'Pairing was rejected by the host')
    .with({ _tag: 'ChannelDisconnected' }, () => 'Connection to remote peer dropped')
    .with({ _tag: 'TimedOut' }, () => 'Operation timed out')
    .with({ _tag: 'CertificateError' }, (x) => `Certificate error: ${x.message}`)
    .with({ _tag: 'BundleNotFound' }, (x) => `Remote client bundle not found at ${x.path}`)
    .with({ _tag: 'NoNetworkInterface' }, () => 'No usable network interface found')
    .exhaustive()
}
