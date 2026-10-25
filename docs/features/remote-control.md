# Native remote host

The Rust application owns the gRPC listener in `AppState.remote`. It serves
application resources, rather than mirroring a window. The domain contract and
release requirements remain in [the plan](../agent-runtime-remote-plan.md) and
[acceptance matrix](../agent-runtime-remote-acceptance.md). Source integration
does not imply native or GUI acceptance. The [Electron reference](remote-control-electron.md)
describes the previous implementation only.

## Starting and stopping

Preferences → Remote access configures an IP address and port. The listener is
disabled by default; the default address is `127.0.0.1:44330` for SSH forwarding.
An explicit LAN/VPN interface or wildcard bind enables direct native clients.
Changing the advertised pairing address does not change the listener binding.

`RemoteState` starts only after storage, the agent environment and Git are ready.
TLS private keys reside in the platform credential store. The database contains
their immutable manifest reference. A missing referenced key refuses startup;
it never silently changes the identity trusted by existing devices. Restarting
the listener reloads the committed manifest. Listener preferences are a bounded,
atomically replaced `remote-host-v1.json` in the existing private data directory.
The single runtime database migration remains unchanged by listener preferences.

`server::start` owns a named thread and Tokio executor. Concrete Chat, Catalog,
Files, Git, Terminal, Tasks, Settings and Artifacts services call the existing
feature owners. The local desktop does not serialize requests to itself. All
services share the same resource-operation admission and file/Git gate.

Quit first refuses new remote effects and waits for accepted operations. It then
ends network subscriptions and stops agent/terminal processes before closing
the metadata worker. A stalled read-only client cannot indefinitely hold Quit.
A later layout/history/cleanup failure keeps Canopy open and restores remote
access. Closing a remote view only detaches it; it does not stop or delete work.

## Pairing and permissions

Create an invitation with the reachable host IP or SSH forward address. The QR
and copied invitation contain the public host certificate, exact SHA-256 pin,
TLS server name, route and five-minute one-time token. The native client creates
its own key and signed PKCS#10 request. Host approval binds the invitation to
that key, device name, explicit scopes and roots. The TLS hostname and pin stay
the same when a client changes LAN/VPN/SSH routes.

Scopes are Read, Chat, Approve, Files/Git, Terminal, Tasks and Admin. Roots are
canonical checkout paths, `private`, or an explicit `*`. Catalog workspace IDs
are grouping identities; they never grant filesystem authority. The device
registry looks up the exact TLS-verified leaf certificate. Request-supplied IDs,
device names and VPN membership cannot authenticate a request.

Preferences lists devices and supports revision-checked permission updates and
revoke. Authorization invalidations wake active subscriptions. Revoke prevents
new effects and disclosures; it does not undo work already accepted by the host.
Accepted chat messages retain their images even if their original sender is revoked.

The remote device registry is keyset-paged by both record count and encoded size;
revoked history is opt-in. Large valid grants cannot make the management RPC
exceed its 1 MiB control budget. Read permits Tasks browsing and safe agent/account
discovery; Tasks is required for provider mutations, and Admin for settings.
Agent discovery never returns environment values, launch arguments or secrets.

An open host watch records live device presence. Project/worktree preflight
distinguishes a connected dirty-buffer owner from a durable offline record.
Connected owners must save or discard on their device; acknowledging offline
risk cannot override a connected device's buffer.

## Commands, streaming and persistence

Mutations carry an authenticated device, command ID and epoch. Typed arguments
are fingerprinted and recorded before external effects. Retry reads the previous
outcome; an uncertain effect is never automatically repeated. ResolveCommand is
an explicit user review of uncertainty, not evidence of success or failure.
Closing an epoch retires it permanently before its terminal receipts are freed.

Chat snapshots register their tail at one boundary. Live data bypasses history
debounce. History pages have a fixed upper key and bounded rows/bytes. Text deltas
use UTF-8 byte offsets; large items/terminal frames use verified fragments.
The persisted checkpoint counter and exact stored-item hashes commit together
with history rows. HistoryCommitted is emitted only after that commit; an error
cannot advertise a failed write as saved. Trimmed content retains its identity
and an explicit incomplete representation.

Standard `grpc.health.v1.Health` is authenticated. Health, host readiness and
HTTP/2 keepalive have separate meanings. Host readiness and authorization events
are pushed; they do not rely on sidebar polling. Settings staging cleanup runs
at startup and every minute on the server executor. Artifact cleanup is owned by
the existing settings lifecycle.

Workspace-root observation starts before listener admission and follows the
application catalog even when no client has opened the catalog screen. Listener
failure changes readiness to StorageFault and drains accepted effects before its
executor exits. Retry reloads the same committed identity.

## Current verification boundary

An isolated test uses real loopback TLS/HTTP2/gRPC, generated certificates and a
temporary database. It checks unauthenticated and unregistered-device denial,
dynamic authorization, health streaming, revoke and listener shutdown. It uses
no GUI, provider account or platform vault. Separate SQLite tests cover checkpoint
rollback and history pagination while new rows arrive. These checks do not prove
macOS/Windows/iOS/Android operation or the release performance matrix.

Client behavior and its verification are described in
[remote-client.md](remote-client.md). macOS testing remains with the user after
implementation. Other native and GUI/E2E cases must be reported as NOT RUN until
they are actually exercised.

The [acceptance handoff](remote-acceptance-status.md) describes the current
evidence boundary and the isolated-data procedure for the user's macOS test.
