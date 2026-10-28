# Native remote host

The Rust application owns the gRPC listener in `AppState.remote`. It serves
application resources, rather than mirroring a window. The domain contract and
release requirements remain in [the plan](../agent-runtime-remote-plan.md) and
[acceptance matrix](../agent-runtime-remote-acceptance.md). Source integration
does not imply native or GUI acceptance. The [Electron reference](remote-control-electron.md)
describes the previous implementation only.

## Starting and stopping

Preferences → Connections (Security group) contains This computer, Devices, Other
computers and a collapsed Advanced settings section; see the UI section below.
Remote access is off by default. The switch retains the existing
listener preference; Local network / VPN listens on IPv4 interfaces, while SSH
only keeps the listener on loopback. Address and port overrides remain under
Advanced settings. Disabling access never quits local agents.

Add device creates a five-minute QR/link with a detected interface address.
Users can select another detected address or an explicit VPN name. A loopback
listener cannot advertise a LAN address; unspecified and multicast destinations
are refused before enrollment. Interface discovery runs on a worker without a
shell or network probe. Test on this computer (Advanced settings) opens the inline
Add computer form under Other computers, prefilled with a fresh invitation; it still
requires an explicit request and approval, with no authentication bypass.

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

Choose Add device to create an invitation for the selected network or SSH route. The QR
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

Approval defaults to **Full access**: all operation scopes, including host
settings, and `*` for every current and future project and private chat. The
All projects and chats control can restrict roots; View only, Chat and Control
presets restrict operations. Control excludes host administration. Existing
devices also have a Share everything action. On the first upgraded startup,
untouched original grants (revision 1, Chat scopes, private only) are widened
atomically. Edited grants, chosen project roots and revoked devices stay intact;
an internal preferences marker prevents the upgrade from running again.

Desktop pairing connects both computers after one invitation and approval.
Adding a computer starts the local host if necessary. Over the authenticated
connection, PeerService exchanges a CSR bound to the pinned host public key;
the local host issues a separate revocable ClientAuth certificate. ConnectPeer
then saves the reverse connection in the existing client registry. It requires
ADMIN on private resources and never accepts an enrollment token as authority.
Deterministic device IDs and existing-certificate lookup make retries idempotent;
they never restore revoked grants or replace an existing connection's drafts,
routes or keys. Each computer then reads the other's normal authorized catalog.
Phones remain clients and do not start a reverse host.

The return credential uses the computer's persisted host key with a distinct,
peer-issued ClientAuth certificate; no private key is exchanged. PeerService
requires that CSR's public key to match the pinned server certificate. A saved
peer connection is restored on application startup. The first upgrade enables
the listener for previously saved connections; later explicit disable choices
are respected. An SSH connection owns a loopback return forward, so the peer
does not need a new inbound network route or SSH credentials.
Preferences lists devices and supports revision-checked permission updates and
confirmed revoke. Authorization invalidations wake active subscriptions. Revoke prevents
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

## Interfejs Preferences → Connections

Strona leży w grupie Security nawigacji Preferences (ikona Globe, jak sekcja Remote
Control w Electronie) i używa tego samego nagłówka co General: tytuł, jednozdaniowy
podtytuł i linia oddzielająca. Treść składa się z sekcji `section`/`setting`.
Błąd pokazuje się przy obszarze, który go wywołał (`integration_message`): lokalne
błędy widoku mają przypisany obszar, a wspólny `RemoteState.error` trafia do obszaru
ostatniego żądania (dostęp, urządzenia, zatwierdzanie, Other computers, Advanced).

- **This computer**: przełącznik Remote access, segment Local network / VPN | SSH
  only oraz wiersz Status (kropka i etykieta: Available, Updating…, Not running
  z akcją Retry).
- **Devices**: lista urządzeń z nazwą, statusem Connected/Offline i plakietkami
  poziomu dostępu oraz udostępnionych zasobów. Permissions… otwiera edytor pod
  wierszem urządzenia; menu ⋯ zawiera Share everything i Remove access…. Usunięcie
  potwierdza wspólny modal `Confirmation` z `wait_for` na `RemoteState`.
  Add device (bez nowego okna) otwiera na końcu listy kartę parowania: QR (stałe `qr_light`/`qr_dark`, szkielet do
  czasu wygenerowania kodu), stan Waiting/Invitation expired, godzinę wygaśnięcia,
  Copy invitation (chwilowo „Copied”), New invitation, Done oraz adres w zaproszeniu
  z listą wykrytych interfejsów. Prośba o dostęp zastępuje kartę parowania kartą
  zatwierdzenia: presety w jednym segmencie, projekty i czaty, zwijane Advanced
  permissions (Disclosure) oraz Deny i Approve z loaderem. Karta parowania i
  zatwierdzenia stoi pod listą, w miejscu przycisku Add device — jak nowy profil
  w AI providers.
- **Other computers**: wiersz z nazwą, plakietką Two-way, statusem
  Connected/Connecting…/Offline, użytą trasą i liczbą udostępnionych projektów.
  Open… (okno workspace komputera) jest widoczne, a Routes i Disconnect są w menu
  ⋯. Routes rozwija pod wierszem kartę z trasami i nazwą komputera na tym urządzeniu
  (Done zwija). Add computer na końcu listy rozwija kartę z formularzem parowania
  (zaproszenie, nazwa urządzenia, SSH z kontrolą klucza, Advanced); Cancel zamyka
  ją i anuluje czekające żądanie, a po zatwierdzeniu komputer dołącza do listy
  i łączy się. Żadna z tych akcji nie otwiera okna. Ładowanie pokazuje spinner,
  pusty stan krótkie zdanie, a błąd połączenia czerwony tekst w wierszu.
- **Advanced settings**: zwijana sekcja (Disclosure) z adresem nasłuchu i Apply,
  adresem w zaproszeniu, wykrytymi adresami z Refresh, Test on this computer,
  odświeżeniem listy urządzeń i Copy diagnostics.

Karta tras (Routes) renderuje listę jako sekcję ROUTES z wierszami w stylu
Preferences: ikona trasy, nazwa z plakietką In use, adres bez schematu `https://`
albo „Through SSH · login@serwer”, Connect, przesuwanie, edycja i usuwanie
potwierdzane modalem `Confirmation`. Formularz trasy jest kartą z segmentem typu
połączenia, siatką pól i polem z kluczem serwera SSH do porównania z właścicielem.
Formularz Add computer i edytor tras to `remote_client::ConnectionSetup`
(`remote_client/inline.rs`): `RemoteWindow` w trybie inline, który renderuje tylko
ten przepływ i zgłasza `ConnectionSetupEvent::{Paired, Closed}`. Parowanie, kontrola
klucza SSH, zapis tras i ich błędy są tym samym kodem co w oknie Remote.

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
