# Native remote clients — implementation notes

This describes source implementation, not a native/GUI acceptance result. The target
contract remains [the remote plan](../agent-runtime-remote-plan.md) and
[acceptance requirements](../agent-runtime-remote-acceptance.md). The older
[Electron remote control](remote-control-electron.md) uses a different protocol.

## Desktop ownership and transport

`src/remote/client/` owns a typed `ClientRegistry` keyed by host UUID. Each host owns
its transport, pinned identity, catalog, connection generation, bounded change
notifications, mutation gate and recovery records. `src/ui/remote_client::open_host`
opens the device window for one computer; adding a computer and editing its routes
use `remote_client::ConnectionSetup` inline in Preferences → Connections. Opening a chat from it selects a normal tab
in the main workspace. The chat uses the same `ui::chat::ChatView`, composer and
conversation renderer as a local chat. Settings/Tasks retain their feature views.
Each window subscribes independently to host changes.

Preferences → Connections shares the retained registry owner with all client
windows. Opening preferences loads saved metadata without connecting or reading
credentials. Other computers shows live status, the route in use and the shared
project count. Open connects explicitly; Routes (row menu) expands the route manager
below the row without a connection.
Add computer expands the enrollment form at the end of the list, also when other
computers are saved; Cancel closes it and cancels a waiting request.
Direct setup shows invitation and device name; SSH fields appear only when chosen.
Successful approval connects automatically using the newly issued identity.

Desktop pairing is mutual: adding A on B also installs B on A, without another
invitation. Both hosts share all current and future projects, private chats,
files, terminals, Tasks and settings by default. The permission editor can still
restrict this. Untouched legacy private-only grants are upgraded once; edited
restrictions are preserved. A connected empty catalog can still mean there are
no open projects visible to this device, rather than a transport failure.

The application host owner restores saved active connections on startup;
opening Preferences itself only reads metadata. Disconnect persists a local
pause. Reconnect enables the connection again. The first startup after upgrading
also enables the local listener for existing saved connections, so they can
establish their reverse direction. Later explicit host-disable choices persist.
The sidebar keeps disconnected/unloaded computers visible with a Connect action.
Older hosts or restricted grants without ADMIN keep their existing one-way
access and show why reciprocal setup is unavailable. Remote access publishes
live catalogs and operates on the host; it does not copy repositories or
synchronize working files.

For SSH, the initiating client owns an additional loopback-only remote forward
to its own gRPC listener. Its port becomes the other computer's SSH return route;
reconnecting refreshes that route without re-pairing. Both directions retain
certificate pinning and mTLS. If the SSH server refuses remote forwarding, the
connection reports that error instead of claiming two-way access. Direct peers
advertise detected interface addresses; every connection attempt still applies
the OS route eligibility checks below. Automatic retries back off to 60 seconds
and stop when the connection is paused or its owner stops.

A desktop computer has up to eight named routes (Home, Office, Tailscale, SSH,
for example). Route order is persisted; Connect tries them in order, and each
route has an explicit Connect action. The list marks the route in use and shows
failed/skipped attempts. Adding, editing, reordering and removing routes keeps
pairing, drafts, journals and permissions. Editing is checked against the version
shown in the form, preventing a stale window from overwriting another edit.
Removing a route is confirmed in the shared Canopy modal; removing the active
route disconnects, and at least one route must remain. Unused SSH
credentials are removed only after the route change commits. The private JSON
format upgrades v1 to v2 on successful save, preserving the previous single route;
older one-route builds refuse v2 instead of dropping the new list. SQLite and
Cargo dependencies are unchanged.

Before desktop TCP/TLS or SSH, Canopy checks the current OS routing table and
active interface. Private, link-local and Tailscale address ranges need a specific
route; an ordinary Internet default does not qualify. Thus an Office route to
192.168.128.0/24 is skipped on a Home-only 192.168.1.0/24 network, while an explicit
VPN route can make it eligible. Linux uses a kernel FIB lookup including policy/VPN
tables; macOS reads BSD route records; Windows uses IP Helper GetBestRoute2.
DNS names resolve before admission and the checked IP is used for transport.
No ping, TCP probe or SSH authentication is sent to an ineligible route. Eligibility
is checked again on reconnect; the UI reports why a route was skipped. Route
fallback performs only authenticated reads and never replays mutations/outbox.
These desktop route checks are source implementation, not native platform proof.

Direct routes use an explicit HTTPS endpoint. SSH routes use an app-owned libssh2
forwarder on an ephemeral loopback port, with a displayed SHA-256 host-key
fingerprint accepted explicitly before credentials are used. Passwords, private
keys and optional key passphrases are stored in the OS credential store. Large
keys are split into bounded vault slots; partial failed writes are removed. No SSH
CLI/configuration/known-hosts edits or agent forwarding are used. The TLS leaf pin
and SNI remain those of the paired host, independent of the SSH route address.
Connection cancellation fences late completions. Disconnect drops the tunnel;
stream loss triggers a bounded reconnect sequence. Catalog and host-readiness
streams share the connection owner. Reconnect never sends queued messages.

Pairing consumes the versioned `canopy://pair` protobuf URI, validates expiry,
token size and certificate digest, creates a P-256 PKCS#10 CSR, polls with the
original token, verifies the returned leaf matches the local key and stores key
and certificate material only in the vault. Only enrollment uses a connection
without a client certificate. Normal RPCs use the paired identity.

## Device persistence and command recovery

The private atomic `remote-client-v1.json` under the runtime data directory stores
host identities/routes, public certificate data, credential references, last-seen
time, cached catalogs/transcripts, device layout and drafts. It does not introduce
an additional SQLite migration. Private attachment files are owned by its upload
journal. Limits bound file size, draft count, cache size, outbox and attachment
storage. Draft revisions prevent delayed workers from replacing newer edits.

The durable outbox accepts only text and images; contextual attachments require
an explicit online send. Dispatch revalidates original host/chat/workspace,
agent/profile and history epoch. Upload Begin/Finish and send envelopes retain
original command IDs across uncertain responses. The generic online mutation
journal stores identity and receipt, not request bodies or credentials. Unknown
outcomes require reconciliation. Reconnect does not replay mutations or move
them to another host.

Explicit reviewed epoch closure queries original IDs, calls `ResolveCommand` for
`delivery_unknown`, persists the reviewed receipt without changing its outcome,
then calls `CloseCommandEpoch`. Interrupted review/closure remains recoverable.
Unsent text survives closure and retired envelopes cannot replay. Unresolved
uploads must be reconciled before closure. Device disconnect/quit never invokes a
host Quit RPC.

## Desktop views

- Catalog: projects/workspaces/chats/terminals, host project opening, chat and
  terminal creation; scoped selection and per-device layout restore.
- Chat: always the shared `ui::chat::ChatView` tab in the main window, driven by the
  client runtime (`app_state/chats/runtime/remote.rs`, `client.rs`). The device window
  and embedded workspace only open it; they no longer render a chat themselves.
- Files: scoped directories/paging/search/create, hash-checked reads, editable
  UTF-8 buffers, private draft restore, dirty presence, expected-hash save and
  confirmed discard. Binary reads can open a private attachment preview.
- Git: status, authenticated bounded diff/history/refs, stage/unstage/discard,
  commit/sign, pull/push/upstream and worktree create/remove/merge/stop. Mutations
  use typed preflight expectations; destructive/worktree actions show impacts for
  confirmation, including explicit discard/offline-dirty flags.
- Terminal: authoritative frames/patches and atomic fragments, fenced input lease
  acquisition/renewal/release, shared keyboard/paste handling, IME, mouse modes,
  selection/copy, input acknowledgement errors, measured resize, read-only history,
  stop/restart and detach. Input is bounded and never replayed after lost ACK.
- Attachments: authenticated chunk download validates identity, offsets, size and
  SHA-256 before publishing a private file; the preview owns its file, reuses the
  native macOS preview/shared image presentation, and offers explicit Save as.

Settings/Tasks forms are implemented in `src/ui/remote_client/settings/` and
`tasks/`; their host contracts and detailed behavior remain documented with
[settings](../settings.md) and [task browsing](../task-browser.md).

## Mobile changes

`mobile/` remains an independent project. Native decisions support explicit tool
allow/deny, question choices/free text, notes and plan approval with accept-edits.
Agent capability names are the typed host names; fork is restricted to a valid
TurnEnd fork point. Close chat detaches; Delete separately preflights and confirms.
The initial earlier-history request uses `before`, later requests use its opaque
page token. Settings use typed host preferences/profile metadata and write-only
secret updates; unchanged secrets are omitted. Removing an integration credential
uses confirmed Disconnect integration, preserving task/project links. Reviewed
command retirement uses ResolveCommand, including uncertain online mutations.
Tasks discover safe integration metadata through the TASKS-scoped ListIntegrations,
without requiring Settings/admin access. Goal state/accounting and typed
pause/resume/clear controls follow advertised capabilities. Contextual attachments
cannot enter the durable outbox; online SendMessage retains the original context,
uses resumable image uploads, and leaves the draft intact on failure.

## Verification boundary

Focused Rust client tests use fake credential stores, typed command closures and
private temporary files; no real vault, host providers or user database. Client
tests cover snapshot/fragment integrity, UTF-8 offsets, enrollment pins,
vault rollback, lost mutation responses, persistence failures, stale draft writes,
independent window notifications and chunked encrypted SSH-key storage.

Mobile `npm run typecheck` passed. `node scripts/test-remote.mjs` passed 29 tests,
including newer-draft preservation during Send, contextual draft retention after
rejected offline enqueue/restart, lost ResolveCommand responses, atomic catalog
pages, bounded diagnostics, terminal input/fragments and upload recovery. These
are not native tests.

Library Clippy with `--locked --lib -- -D warnings` passed at the client checkpoint.
Final combined verification and the macOS procedure are tracked in the
[acceptance handoff](remote-acceptance-status.md). No GUI/E2E, native iOS SSH/module compilation, macOS interaction,
live TLS/SSH/provider flow or real-vault test was run. Source/type checks do not
verify visual parity or native end-to-end behavior; those remain acceptance checks.

### Replayable chat operations and authorized pages

`ChatMutation` is the explicit non-secret client allowlist for online Send, Edit, Fork,
RunShell and StopShell. `submit_chat_mutation` uses the existing mutation gate and
atomically saves the original command plus encoded request before RPC. The device
can inspect `chat_mutations(chat)` and explicitly call `retry_chat_mutation(id)`.
Recovery queries the original receipt; only NOT_FOUND in the still-open epoch
permits byte-identical replay. Accepted, attempting, succeeded and unknown results
are not redispatched. Generic mutations continue to store identities only, so
settings credentials never enter this replay journal. Saved Edit recovery protects
its previous draft's local images from attachment cleanup.

`HostClient::read_chat_page` consumes the authorized catalog page stream, validates
its immutable artifact identity, HTML MIME, 32 MiB bound, contiguous progress,
64 KiB chunks, completion and SHA-256, then returns HTML bytes to the device's
isolated preview. It never navigates to a host loopback URL. Mobile uses the same
contract with private native staging and a sandboxed iframe. See
[mobile source handoff](../../mobile/NATIVE-REMOTE.md) for the R13 mobile behavior
and explicit native/GUI NOT RUN status.

The [desktop view handoff](../../src/ui/remote_client/README.md) lists the device
surfaces. Agent discovery accumulates bounded pages independently of Send. Closing
workspaces expose Retry cleanup and cannot receive new chats. Notes retain the
existing process-local, device-local workspace scratchpad semantics; export writes
an explicitly named new host file rather than an invented shared path.

## Interfejs okna Remote i narzędzi zdalnych

Okno „Canopy — Remote”, zdalny workspace w głównym oknie i wąski panel narzędzi
korzystają z tych samych komponentów co Preferences i lokalne Files/Changes
(`src/ui/remote_client/shell.rs`, `chrome.rs`). Kod nie zmienia `src/remote/**` ani
`src/app_state/**`.

- Nawigacja jak w Preferences: sekcja COMPUTERS z kropką statusu i stanem
  „Connected” / „Connecting…” / „Offline” (nigdy `{:?}`), „Add computer…”, a po
  wybraniu komputera WORKSPACE (Projects, Files, Changes, Notes, Tasks) i COMPUTER
  (Connection, Host settings). Pozycje są przyciskami z focusem i nazwą dostępności.
- Nagłówek strony: tytuł, podtytuł (checkout · komputer) i dolna linia; po prawej
  stan połączenia, Connect/Reconnect i Disconnect (Disconnect zatrzymuje też
  automatyczne ponawianie offline). Pod nagłówkiem: informacja o pracy offline na
  zapisanych danych, ostatnie połączenie i błąd połączenia.
- Komunikaty są przy kontrolce, która je wywołała: `run_in(scope, …)` pokazuje
  spinner na tej kontrolce i błąd/wynik obok (`notice(scope)`); stare jednolinijkowe
  pole statusu na dole okna zostało usunięte. `run` bez zakresu (trasy, terminal)
  zostaje komunikatem strony. Nawigacja czyści komunikaty.
- Stany puste mają jedno zdanie i akcję (np. „Open Projects”, „Connect”), a ładowanie
  spinner z wyciszonym tekstem.
- Projects: checkouty jako wiersze z gałęzią, czaty (otwierają zwykły tab ChatView),
  terminale ze stanem, start terminala, New chat (miejsce docelowe, wybór agenta i
  konta jako lista z zaznaczeniem, „Create chat”) oraz otwarcie projektu po ścieżce.
- Files: wiersze w geometrii lokalnego drzewa plików, pasek folderu (w górę, przeładuj,
  nowy plik/folder przez `TextPrompt`, który od razu odświeża folder), wyszukiwanie,
  edytor z Preview…/Discard edits…/Save. Szeroki układ: lista obok edytora; panel
  narzędzi: lista nad edytorem. „Mark as edited” jest w sekcji ADVANCED.
- Changes: litery statusu i wiersze plików jak lokalny panel
  Changes (Stage/Unstage/Discard jako ikony), skrócone hashe drugoplanowo, panel diffu,
  commit z podpisem, Pull/Push, a remote/upstream i worktree w sekcjach
  „REMOTE AND UPSTREAM” i „WORKTREES” z opisami zrozumiałymi bez żargonu.
- Host settings: sekcje jak w Preferences (preferencje, agenci, profile, integracje z
  ikoną dostawcy, urządzenia), edytor w karcie z polami w dwóch kolumnach i sekretami
  Keep/Replace/Remove. Niepewne operacje, ich czyszczenie i zakresy urządzeń są w
  sekcji DIAGNOSTICS.
- Potwierdzenia korzystają z `components::modal::Confirmation` z czasownikiem
  wyniku; nieodwracalne akcje mają styl destrukcyjny (`Confirmation::destructive`).
  `TextPrompt::confirm_label` nazywa akcję zapisu w prompcie.
- Podglądy (załącznik, tekst/Markdown, HTML) mają przeciągalny pasek tytułu z akcjami
  i komunikaty błędów w stylu aplikacji.
- Strony Terminal, lista terminali i Tasks dostają całą wysokość obszaru treści (bez
  zewnętrznego scrolla i wcięcia); lista terminali dodaje własne wcięcie.
  `RemoteWindow::new_shell_terminal` startuje powłokę w wybranym checkoucie.

Martwy zdalny stos czatu (`remote_client/chat.rs`, `editing.rs`, `activity.rs`) usunięto:
nic nie ustawiało `Page::Chat` poza odtworzeniem układu, które od razu przekazuje czat
do ChatView. Wariant `Page::Chat` zostaje wyłącznie dla zgodności zapisanych układów.

Weryfikacja: `cargo build --locked`, `cargo clippy --locked --all-targets` bez
ostrzeżeń, `cargo test --locked --bin canopy-desktop remote_client` (7 z 8 przechodzi;
`selecting_another_computer_while_the_registry_loads_keeps_the_requested_destination`
nie przechodził już przed zmianą — brak `AppState` w teście). GUI nie zostało
sprawdzone w oknie.

## Wspólny sidebar komputerów

Główne okno pokazuje lokalny katalog razem z katalogami zapisanych połączeń.
Projekt jest grupą prezentacji według `origin`: SSH i HTTPS tego samego serwera
i repozytorium tworzą jeden wpis, np. `itsoltech/canopy-desktop`. Nazwa serwera
pozostaje częścią klucza, więc GitHub i GitLab nie zlewają różnych repozytoriów.
Brak rozpoznawalnego origin pozostawia osobny projekt danego komputera i ścieżki.
Do katalogu trafia wyłącznie oczyszczona tożsamość origin, bez loginu, hasła,
parametrów URL i fragmentu. Lokalne ścieżki i względne repozytoria SSH użytkownika
nie są podstawą grupowania między komputerami.

Worktree i czaty zachowują identyfikator komputera. Dwie gałęzie `main` ani czaty
o tej samej nazwie nie są scalane. Wiersze podają środowisko, a niedostępny host
zachowuje ostatni kompletny katalog. Kliknięcie zdalnego czatu otwiera lub wybiera
zwykły lokalny tab. `PaneMetadata.remote` przechowuje UUID hosta i czatu;
przeciąganie, splity, fokus i persist/restore korzystają z tego samego modelu
workspace co lokalne rozmowy. Zdalne pliki i narzędzia nadal mają osobne adaptery.
Główne skróty tworzenia czatu, zapisu i zamknięcia działają w wybranym środowisku;
zamknięcie widoku zdalnego nie usuwa czatu na hoście.

`ChatRuntime` z klientowym źródłem nie uruchamia lokalnego agenta, nie zapisuje
historii do lokalnej bazy hosta i nie uczestniczy w jego GC ani Delete. Odwrotna
projekcja w `remote::client::presentation` udostępnia wspólne karty narzędzi,
zgody, wiadomości, wyniki shella i cele. Wywołania używają oryginalnego hosta,
epoki i punktu rozmowy. Zwykłe zamknięcie lokalnego taba również pozostawia czat
w katalogu; jawne Delete i zamknięcie projektu zachowują osobne potwierdzenia.

Nowe pola katalogu (`ProjectSummary.origin`, `WorkspaceSummary.branch`) są
opcjonalne. Starszy host nadal działa, ale do grupowania po origin oraz pokazania
aktualnej gałęzi potrzebuje aktualizacji. Zmiana lokalnych metadanych Git odświeża
katalog subskrybentów. Projekcje klienta są współdzielone przez `Arc`, bez kopii
całego katalogu przy renderowaniu sidebara.
