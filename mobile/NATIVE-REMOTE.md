# Native remote implementation and handoff

This is unreleased source implementation of the native mobile path. It is **not a
completed E0/RPC-07 or E5 acceptance result**. The app's Instances, pairing, host,
chat, workspace, terminal, Tasks and host-settings routes now call native gRPC.
They no longer start the legacy WebRTC session. Legacy code and saved instance
records remain on disk; they are not silently adopted into a new host/device
identity. An existing OTA-only binary must be rebuilt with the local Expo module.
Expo Go and web have no fallback transport.

## Native transport and credentials

`modules/canopy-remote` is discovered by Expo's local-module autolinking. The podspec
is in `ios/`. Existing JavaScript dependency versions and package-lock were not
changed by this work.

- Android uses grpc-java/OkHttp **1.71.0**. iOS uses gRPC-Core **1.71.0** and its
  native completion queues. Both use the generated method catalog, raw Protobuf,
  unary/server-stream/bidi, deadlines, cancellation and independent HTTP/2
  keepalive. TLS checks the expected server name and exact pinned leaf; ordinary
  RPCs also present the paired device certificate. Automatic gRPC retry is off.
- Native receive flow control allows one unacknowledged message per call. JS ACK
  follows successful consumer application. Limits are 32 calls/host, 1 MiB per
  wire message, 4 MiB queued inbound/host and 4 MiB JS outgoing. A call has only one
  pending write. Subscription deadlines are one hour; ending a host stream
  triggers bounded reconnect, not replay of mutations or terminal bytes.
- Android SSH uses **SSHJ 0.39.0**. iOS SSH uses **libssh2-iosx 1.11.1.0** with
  **openssl-iosx 3.5.9.2**. The app owns loopback sockets and the forwarding worker;
  the host-key SHA256 pin is checked before password/private-key authentication.
  iOS serializes libssh2 calls on one worker with two 32 KiB buffers. Disconnect
  joins the worker and frees sockets/channel/session. No shell, known_hosts,
  SSH agent or external tunnel process is used. Canopy TLS remains inside SSH.
- Device CSR creation uses JCA/BouncyCastle **1.80** on Android and
  **BoringSSL-GRPC 0.0.39** on iOS. Device keys stay in the native module: JS gets
  the CSR and passes credential references to connect/installIdentity. iOS uses
  Keychain `WhenUnlockedThisDeviceOnly`; Android encrypts app-private non-backup
  records with an AndroidKeyStore AES-GCM key and checks device unlock state.
  There is no native credential export/read method exposed to JS. User-entered
  SSH credentials are handed to the vault once, not put in host state files.
- Earlier unreleased JS SecureStore credentials are retained but not read or
  automatically migrated by the new native vault. Re-pair explicitly if using a
  development identity created by that earlier implementation.

## State and actual routes

- `RemoteHosts`/`HostSession` own each paired host independently. Registry and
  host/device-local state use atomic non-backup files. Host service readiness
  (`grpc.health.v1.Health` Check/Watch), WatchHost domain state and connection
  phase remain distinct. Backgrounding cancels streams and closes channels and
  SSH. Foreground reconnect queries existing commands and restores subscriptions;
  it never automatically sends offline composer messages.
- Instances lists real native hosts. Host view exposes the typed catalog,
  projects/checkouts/private chats, agent/profile creation, terminal navigation,
  connection route selection and adding LAN/VPN or pinned SSH routes. Views carry
  host plus resource IDs. Scroll state and file/composer drafts are per device;
  they are not sent as desktop navigation or terminal scroll mutations.
- QR view uses `PairingClient`, keeps a pending reference for explicit continuation,
  polls the original CSR/token with cancellation, and persists the registry before
  discarding enrollment data. Completion can be retried after a lost response.
- Chat view consumes SubscribeChat with `ChatEventAssembler` and
  `ChatProjectionOwner`. A staged snapshot becomes visible only at SnapshotEnd;
  UTF-8 append offsets/revisions and sequence boundaries are checked. Large items
  are hash-verified up to 32 MiB with <=64 KiB fragments. ReadHistoryPage is a
  stream with a separate read boundary and the same fragment checks. Cancelled or
  incomplete pages never modify the visible transcript. History search, queue
  edits/pause/move/removal, opaque decision options, agent options, fork, goal,
  steering, interrupt/restart and background Stop have real typed actions.
- Composer text, native staged images and structured path/quote/command/skill
  attachments are persisted before Send. Offline messages require explicit Send
  after reconnect. Context includes agent/profile/history/workspace; the original
  draft context is retained across changes. An account mismatch is rejected and
  a history/workspace change requires explicit confirmation.
- `ClientOutbox` retains immutable command/message IDs and epoch before the wire
  request. GetCommand reconciliation handles lost ACKs; NOT_FOUND is not proof of
  nonexecution. Unknown host-queue delivery has explicit resolution choices.
  Unresolved records cannot silently disappear. Failed/rejected/retired messages
  can be restored as a new draft. Closing the epoch is explicit and does not replay
  requests or erase unknown text. Outbox is capped at 100 entries and 64 MiB of
  text/metadata.
- Attachments use native document pickers and app-private staging. Sources are
  <=64 MiB, <=10 images/message and <=256 MiB staged bytes. Upload journals retain
  Begin/Finish command IDs and received-range reconciliation. PutChunk is <=64 KiB;
  ready artifact size/hash must match before Send. Native download verifies hashes
  before image/text preview. Other media preview coverage is listed below.
- Workspace view performs paged directory listing/search/create and text editing.
  Save uses a ready artifact and original base hash. Conflicts retain the local
  buffer; explicit reload discards it. Successful Save adopts only its returned
  hash and preserves edits made while saving. UTF-8 BOM and CRLF are retained.
  Dirty presence carries only identity/base hash and is refreshed while editing.
- Git uses typed status/diff/history/refs and root-owned Preflight. The exact
  operation intent is reviewed with its impacts and expiry, and the returned
  preflight ID/snapshot is sent on mutation. Stage/unstage/discard, commit/sign,
  Pull/Push and worktree create/remove/merge/Stop use this path.
- Terminal view renders host cells, independent stable wire flags, Unicode wide
  spacers and color/style values. It owns an input lease and 5 s renewal, sends
  run/fencing token and monotonic input ID, and waits for queued ACK before more
  input. It never replays input after an uncertain ACK. Paste and arrow/Enter keys
  follow the run's wire modes. ReadHistory stages a complete read-only viewport
  and installs it only at the matching end boundary; it does not scroll the host.
- Host settings use typed agents/profiles/integrations, environment changes and
  LeaveUnchanged/Set/Remove secret actions. Devices can be listed/revoked. Generic
  online mutations retain receipt identities (not bodies or credentials) before
  dispatch; unknown outcomes block new effects until explicit recovery. Operation
  recovery and command-epoch closure are visible in host settings.
- Tasks use typed project identity (including workspace), provider schemas and
  choices, required workflow forms, filters, field edits, comments, labels,
  assignees, milestones, links, worklogs, watching/voting, attachments, board/sprint
  selection and task-to-worktree. Status IDs are chosen from host responses, never
  supplied as a made-up generic status string. Unsupported provider capabilities
  and field read-only reasons are displayed. Standard Protobuf Value/Struct maps
  transport provider field values; they are not a generic command endpoint.

## Generation and validation

```sh
node mobile/scripts/generate-remote.mjs
node mobile/scripts/generate-remote.mjs --check
npm run typecheck --prefix mobile
node mobile/scripts/test-remote.mjs
ruby -c mobile/modules/canopy-remote/ios/CanopyRemote.podspec
```

The generator reads protoc's descriptor set including imported standard messages,
map entries, nested enums and Health. It generates TS and both native method
catalogs together. int64/uint64 remain decimal strings. The codec enforces oneof,
message/depth limits and correct map wire entries, including non-prototype map keys.

Checks performed on Linux during this implementation:

- PASS: complete Expo project TypeScript check with the pinned TypeScript 7.0.2
  and the dependency versions in package-lock, installed with scripts disabled in
  `/var/tmp/canopy-mobile-fullcheck`. `mobile/node_modules` is an ignored symlink
  to that temporary check installation. No package version or repository lockfile
  was changed. Direct `npm ci` initially refused the existing ws override mismatch
  (locked 8.21.0 versus resolver 8.22.0); the temporary manifest pinned its override
  to the already-locked 8.21.0 to reproduce the existing graph.
- PASS: 14 focused Node regression tests, including snapshot atomicity/UTF-8
  conflicts, fragmented history boundary, upload restart/lost Begin/Finish ACK,
  standard Struct map interoperability, and fenced mode-aware terminal input with
  no replay after lost ACK, and fragmented terminal viewport/history atomicity with corrupt digest rejection. These extend the earlier outbox/transport tests.
- PASS: protoc generation/freshness and Ruby podspec syntax.
- NOT RUN: CocoaPods resolution, Xcode/iOS build, Android Gradle/JDK/SDK build,
  native CSR/TLS/mTLS/SSH, device vault persistence, native pickers, device touch/
  keyboard, live host/providers, GUI, EAS, simulator/emulator and E2E. No native
  compilation or end-to-end functionality is inferred from TypeScript/tests.

## Remaining release work

The host's typed handlers are being implemented independently. A source-level
control invoking an RPC is not evidence that the full host/domain/provider path
has passed acceptance. The full iOS/Android/macOS/Windows matrix and AC-19/27/31/32
remain NOT RUN. E0/RPC-07 and E5 remain incomplete until native integration and
required feature acceptance are demonstrated.

Native rich media/font/video/HTML rendering, Notes interaction, PR discovery,
provider-specific form behavior, exact visual comparison and touch usability
require device validation. Their source workflows are described below.
Worktree destructive approvals and attachment cleanup also need device validation;
cleanup retains active and removed-host recovery references and removes
unreferenced staged files only after a one-hour grace period. No feature is
removed from the runtime plan by this handoff.

Sources inspected: [libssh2 1.11.1 headers](https://github.com/libssh2/libssh2/blob/libssh2-1.11.1/include/libssh2.h),
[libssh2-iosx 1.11.1.0 podspec](https://github.com/apotocki/libssh2-iosx/blob/1.11.1.0/libssh2-iosx.podspec),
[gRPC Core credentials](https://github.com/grpc/grpc/blob/v1.71.0/include/grpc/credentials.h),
[grpc-java OkHttp](https://github.com/grpc/grpc-java/blob/v1.71.0/okhttp/src/main/java/io/grpc/okhttp/OkHttpChannelBuilder.java),
[SSHJ 0.39.0](https://github.com/hierynomus/sshj/blob/v0.39.0/src/main/java/net/schmizz/sshj/SSHClient.java),
and resolved Expo Modules Core 56.0.17 native module/activity-result sources.

Terminal frames use the documented complete/patch contract. Live fragments advance
received sequence while the committed cursor advances only after SHA-256 verification
and viewport installation. History fragments stay private until the matching end
boundary. Both paths enforce 64 KiB chunks, 32 MiB assemblies, contiguous offsets,
identity, no interleaving, and cancellation cleanup through native hashing.

## R13 source completion and regression coverage

The follow-up implementation addresses the R12 mobile findings in source:

- Chat and terminal subscriptions own cancellable exponential backoff (100 ms to
  2 s). Transient read faults do not exhaust a fixed attempt count. Disposal
  cancels retry and stream ownership immediately; terminal input is never replayed.
- File Reload reads/validates the complete replacement before one atomic state
  write. Read failures, cancellation, disk failure and edits made during the read
  retain the previous durable draft. The replacement itself is durable before
  the visible buffer adopts it.
- Search retains typed anchors, page cursors and incomplete flags. Selecting a
  result reads a bounded history page and scrolls to that item. The transcript
  navigator and return-to-live action are independent of the host view. Fork has
  explicit private/checkout destination choices.
- Terminal controls cover Escape, Backspace/Delete, Insert, Home/End, Page keys,
  F1–F12 and Shift/Control/Alt/Meta sequences. A native Expo view captures hardware
  keys in the focused terminal region; the normal input field retains IME text.
  Input uses current cursor/newline/paste modes. The cursor is drawn in its cell.
- New-chat discovery uses READ-scoped ListAgents and exact account IDs, including
  explicitly advertised native accounts. Command/skill/model discovery and PR
  choices use the new typed catalog APIs, not administrative GetSettings or
  caller-invented prompt metadata.
- Project close presents the host preflight, runtimes, queues, viewers and dirty
  devices before CloseResource. Failure remains visible and retryable. There is
  no remote Quit action. Closing checkouts remain visible with Retry cleanup; new chats and forks cannot target them.
- Tasks stores non-secret draft fields under host/device/workspace/integration/
  project/task identity. Draft restore entries recover title/body/comment/forms;
  link removal and schema/activity/options pagination call typed provider RPCs.
- Semantic tool cards show status, activity, elapsed time, commands, structured
  plans, diffs, authorized file actions and subagent thread navigation. Unknown
  tool details remain inspectable. Interactive chat pages use ReadChatPage,
  immutable artifact validation, private staging and a sandboxed iframe without
  same-origin privileges, native messaging, credentials, navigation or networking.
- Sent-message Edit uses PreviewEdit including original context/images and first
  message support; file rewind is opt-in with path/change preview. Its separate
  durable replacement draft survives errors and leaves the unsent composer intact.
  Shell is an explicit composer mode, with run-bound Stop on its transcript card.
  Normal Send treats a leading `!` literally. Edit/Fork/RunShell/StopShell retain
  original typed requests before dispatch. Explicit recovery queries the original
  command and replays only NOT_FOUND in the open epoch, with the identical ID/body;
  accepted/attempting/unknown outcomes never cause automatic redispatch.
- Notes follows the Electron reference: device-memory project/worktree scratchpads
  survive view navigation until process exit, have an editable Markdown preview,
  and explicitly export through conditional Files operations. Notes do not invent
  host paths or claim automatic restart persistence. Existing image/video/audio/
  font/PDF views remain source implementations; HTML file previews now share the
  isolated renderer rather than loading privileged local page content.

Current R13 checks: full mobile TypeScript check PASS, generated contract freshness
PASS, podspec Ruby syntax PASS, 22 focused Node regressions PASS (including extended
read recovery, reload failure retention, terminal key encoding, original-ID chat
recovery and corrupt HTML cleanup). This supersedes earlier test counts above.
Native iOS/Android compilation, keyboard/touch/IME/media rendering, vault/TLS/SSH,
GUI/E2E, live providers and AC-30/platform qualification remain **NOT RUN**. Source
coverage and these isolated tests are not a native acceptance result.
