# Windows: audyt i plan implementacji

Status: **implementacja etapowa**. Audyt źródeł wykonano 2026-09-14 na
`c6edbe33ddbc60c961c8ef158d921db638842f7c`. Etap W0/W1 dodał natywny runner
MSVC, checklistę w `docs/windows.md` oraz przenośny relay hooków z backendem named
pipe. Implementacja W2 dodaje politykę shell/resolve, jawne adaptery skryptów,
cytowanie programu ConPTY oraz atomowy Job Object. Implementacja W3 dodaje
wspólne katalogi platformowe, Windows Credential Manager i prywatne profile
agentów. Implementacja W4 dodaje nadzorowane hooki/signery oraz dokładny lookup
poświadczeń HTTPS. Implementacja W5 dodaje natywne nieaktywujące okno notcha,
region inputu i monitory Win32. Implementacja W6 dodaje caption controls,
bezkolizyjne skróty, AltGr/IME, font UI i systemowy Reduce Motion. Bieżąca
implementacja W7 wzmacnia granice ścieżek/reparse points, zapis atomowy, watchery,
cleanup worktree oraz Windowsowe formaty argv i dropu.
Natywne testy i GUI Windows pozostają do wykonania; bieżący stan każdej funkcji
i ograniczenia są prowadzone w checkliście.

## 1. Rezultat i zakres

Docelowo Canopy ma działać natywnie na Windows razem z terminalami, agentami,
notchem, Git, integracjami, edytorem, podglądami i trwałym stanem. Samo uzyskanie
pliku EXE lub odsłonięcie modułów przez `cfg` nie zamyka tego zadania.

Proponowana pierwsza kwalifikowana platforma: **Windows 11 x64,
`x86_64-pc-windows-msvc`**, lokalna sesja desktopowa. Windows 10, ARM64, WSL,
RDP i wszystkie konfiguracje GPU nie są domyślnie objęte deklaracją wsparcia.
Sprawdzić minimalne wymagania rozwiązanych zależności przed opublikowaniem
macierzy wsparcia. WSL nie zastępuje natywnego backendu Windows.

Założenie produktu dla notcha: ta sama wyspa sesji przy górnej krawędzi
głównego monitora, bez zależności od fizycznego wycięcia ekranu. Windows nie ma
macOS-owego menu bar; wysokość belki jest tokenem produktu, a nie różnicą
`visible_bounds` i `bounds`. Zachować zawartość, gęstość, filtry, kolejność sesji,
animacje i kierowanie do pane'a. Nie robić redesignu całej aplikacji.

Plan jest podzielony na zadania W0–W9 z właścicielami plików, zależnościami,
testami i bramkami odbioru. Nazwy nowych modułów są propozycjami, a nazwy Win32
punktami do sprawdzenia w API; nie są deklaracją gotowej integracji.

## 2. Audyt obecnego kodu

Kategorie: **B** — blokada kompilacji widoczna w źródle; **F** — brak funkcji
lub jawny błąd poza macOS/Unix; **R** — ryzyko semantyki wymagające kwalifikacji.

| Obszar | Dowód / wejście do kodu | Wniosek dla Windows |
| --- | --- | --- |
| Relay agentów | `src/agents/relay.rs`: bezwarunkowe `os::unix::{fs::PermissionsExt, net::{UnixListener, UnixStream}}`; `tempdir_in("/tmp")`; `src/agents/mod.rs` eksportuje relay | **B**. Potrzebny transport Windows zarówno w serwerze, jak i `--agent-hook`; samo ukrycie notcha nie naprawi kompilacji. |
| Notch | `src/app.rs:70`, `src/ui/mod.rs:16`, `src/ui/components/mod.rs:81`, `src/ui/theme.rs:149` | **F**. Start, kontroler, motion, komponenty i token `notch_idle` są gated na macOS. |
| Native notch | `src/ui/notch_macos.rs`: `anchor_to_screen_top`, `PointerMonitor`, `InputRegion`; `src/ui/notch.rs::open` | **F**. NSPanel, NSEvent i NSScreen nie mają backendu Windows. Czysta geometria jest obecnie umieszczona w pliku AppKit. |
| Shell / wykrywanie narzędzi | `src/terminal/environment.rs::user_shell`, `probe`, `resolve` | **F/R**. `probe` poza Unix zawsze zwraca błąd; `SHELL`/passwd, PATH i sprawdzanie executable są uniksowe. Brak PATHEXT i obsługi `Path` bez względu na wielkość liter. |
| Argumenty shella | `src/state/tools.rs::ToolCatalog::defaults`, `src/terminal/session.rs::LaunchSpec::for_pane` | **F/R**. Domyślne `-l` nie jest uniwersalnym argumentem PowerShell/cmd. Dwa miejsca wymagają spójnej polityki. |
| PTY | `src/terminal/session.rs`, `spawn.rs`, `clear_scrollback.rs` | Istnieje `escape_args: true` dla Windows, Alacritty ma ConPTY. **R**: zatrzymanie grupy procesów i cleanup potomków są tylko Unix. SIGWINCH i adapter macOS E3 pozostają platformowe. |
| Hook command / argv / drop | `src/agents/launch.rs::augment`, `src/terminal/file_drop.rs`, `src/integrations/task_context.rs`, `src/ui/preferences/tools.rs` | **R**. POSIX single-quote i `shell_words` nie definiują poprawnego cmd/PowerShell ani formatu referencji pliku dla każdego agenta. |
| Prywatna konfiguracja Gemini | `src/terminal/agent_config.rs::overlay` | **F**. Inne pliki profilu są linkowane tylko pod `cfg(unix)`; poza Unix pętla je pomija. OAuth i pozostała konfiguracja mogą zniknąć z prywatnego profilu. HOME też jest założeniem Unix. |
| Klucze API agentów | `src/terminal/credentials.rs` | **F**. Store/load poza macOS zwracają błąd; remove jest pusty. |
| Tokeny integracji | `src/integrations/credentials.rs` | **F**. Store/load/remove wymagają macOS. Dotyczy GitHub, Jira i YouTrack korzystających ze wspólnego serwisu. |
| HTTPS Git | `src/git/network.rs::callbacks` | **F**. Hasło pobierane tylko z macOS internet Keychain; poza macOS pozostaje komunikat o Keychain. Publiczny remote nie jest dowodem działania uwierzytelniania. |
| SSH Git | `src/git/network.rs`, `src/git/ssh_credentials.rs` | **R**. HOME dla `.ssh`; zgodność vendored libssh2 z Windows OpenSSH agent nie jest udowodniona. Nie zakładać, że działający `ssh.exe` oznacza działający `Cred::ssh_key_from_agent`. |
| Hooki Git | `src/git/hooks.rs::run` | **F/R**. Runner po spawnie odrzuca nieuniksowe pipes, fallback używa `/bin/sh`; cleanup grupy jest Unix. Końcowy `drain` na blokującym Windows pipe może dodatkowo zatrzymać worker. |
| Podpisy commitów | `src/git/signing.rs` | **R**. Spawn i czytniki są częściowo przenośne, ale `setsid`/kill grupy nie; potomek trzymający pipe może blokować join po timeout. `~/` używa HOME. |
| Baza i task-context | `src/app_state.rs::SettingsState::new`, `src/integrations/task_context.rs::prepare` | **F/R**. Powielona ścieżka `HOME/Library/Application Support/Canopy Rust`; `CANOPY_DATA_DIR` jest obejściem developerskim, nie implementacją Windows. |
| Prywatność plików | `src/settings/database.rs`, `src/integrations/attachments.rs`, `task_context.rs`, `terminal/agent_config.rs` | **R**. Tryby 0700/0600/0400 są Unix. Sprawdzić ACL i lifecycle Windows, także otwarte uchwyty blokujące usunięcie. |
| Video | `src/ui/video_native.rs`, `video_preview.rs`, `native/video_preview.m`, `src/app_state/editors.rs` | **F**. AVPlayer/AVPlayerLayer tylko macOS. Files nadal tworzy `PaneKind::Video` na Windows, a `Editors::sync` pomija utworzenie widoku. Brak poprawnego playera, ryzyko trwałego pustego/loading pane'a. |
| Podgląd załączników | `src/ui/attachment_native.rs`, `attachment_preview.rs`, `native/attachment_preview.m` | **F**. Quick Look tylko macOS; po pobraniu Windows pokazuje komunikat o braku podglądu, zachowując Save. |
| Motion | `src/motion/mod.rs::policy` | **F/R**. Systemowy Reduce Motion jest czytany z NSWorkspace; poza macOS tylko lokalne `cx.reduce_motion()`, bez widocznego podłączenia do ustawienia Windows. |
| Fonty | `src/ui/theme.rs::init`, `terminal_font`, `src/app.rs` | **R**. UI wymusza `.SystemUIFont`. JetBrains Mono jest embedded, ale pozostałe fallbacki zależą od fontów systemu. |
| Skróty | `src/app.rs`, `src/ui/mod.rs`, `src/ui/terminal/mod.rs`, Preferences | **R**. `cmd-*` nie mapuje się automatycznie na Ctrl; widoczne `⌘` i copy o Keychain są macOS-owe. |
| Titlebar / okna | `src/ui/components/titlebar.rs`, `src/app.rs`, `src/ui/preferences.rs`, okno attachment preview | **R**. Wspólny drag helper istnieje, ale trzeba dodać/zweryfikować Windows caption controls, Snap, resize i offsety po traffic lights. |
| Watchery i pliki | `src/files/tree_watch.rs:155`, `src/app_state/files.rs:550`, `src/files.rs`, `src/git/*watch*` | Jest osobna gałąź innych backendów, nie pusta implementacja. **R**: ReadDirectoryChangesW, rename/save, katalogi tracked, junctions, case sensitivity i długie ścieżki. |
| Worktree / usuwanie | `src/git/worktree_identity.rs`, `removal.rs`, `worktree_workflow.rs`, `src/state/projects.rs` | **R**. Ścieżki Windows, reparse points, read-only i sharing violations muszą zachować istniejące potwierdzenia oraz walidację tożsamości. |
| Build / dystrybucja | `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `build.rs`, `scripts/build-macos.sh`, `.cargo/config.toml` | Toolchain 1.95.0; GPUI Kit 0.6.0 / gpui-pre 0.3.4; Alacritty 0.26.0. Objective-C jest poprawnie gated, lecz nie ma skryptu pakowania Windows ani katalogu `.github` z CI. |
| Testy | `tests/agents.rs`, `terminal.rs`, `agent_settings.rs`, `git_changes.rs`, `commit_hooks.rs`, `files.rs`, `git.rs` | **B/R**. Są bezwarunkowe importy Unix w testach agentów, fixture `/bin`, `/tmp`, shebang, symlinki i całe zestawy `cfg(unix)`. Zielony test suite macOS nie kwalifikuje Windows. |

Nie należy przepisywać przenośnych modeli workspace, splitów, sesji, SQLite,
klientów HTTP, parserów, image/font preview ani operacji Git na shell.
Audyt nie wykazał CEF w aktualnym `Cargo.toml`, `Cargo.lock` ani `src/`;
nie dodawać migracji historycznej przeglądarki do tego zakresu.
Metryki/mocki Preferences nie stają się działającymi funkcjami przez ten port.

### Sprawdzone ograniczenia zależności

Źródła czytano z lokalnego Cargo registry, dla wersji w lockfile. Ścieżki
poniżej są względem katalogu pakietu, aby kolejny agent nie zależał od HOME autora.

| Pakiet / plik | Sprawdzony fakt | Konsekwencja |
| --- | --- | --- |
| `gpui-pre-windows-0.3.4/src/window.rs:471` | PopUp ustawia `WS_EX_TOOLWINDOW | WS_EX_TOPMOST`; brak `WS_EX_NOACTIVATE` w tej gałęzi | Sam `WindowKind::PopUp` nie realizuje kontraktu notcha. |
| ten sam plik, ok. 559 | `focus: false` stosuje `SW_SHOWNOACTIVATE` dla początkowego pokazania | Nie dowodzi braku aktywacji przy późniejszym kliknięciu. |
| `gpui-pre-windows-0.3.4/src/events.rs:96` | `WM_MOUSEACTIVATE => MA_ACTIVATE` | Potrzebne przechwycenie tylko dla okna notcha. |
| `gpui-pre-windows-0.3.4/src/window.rs:885` | Jest obsługa `WindowBackgroundAppearance::Transparent`, backend używa DirectComposition | Wymagany test alfy i hit-testu; nie dokładać bez sprawdzenia layered-window flag kolidujących z rendererem. |
| `gpui-pre-0.3.4/src/platform/keystroke.rs:143` | `secondary` = Cmd na macOS, Ctrl gdzie indziej; `cmd/super/win` = platform modifier | Używać semantycznych skrótów z wyjątkami dla terminala. |
| `gpui-pre-0.3.4/src/platform.rs:892` | Domyślna metoda `titlebar_double_click` jest pusta | Sprawdzić natywną drogę Windows hit-test/WM_NCLBUTTONDBLCLK, nie obiecywać działania na podstawie helpera. |
| `alacritty_terminal-0.26.0/src/tty/windows/mod.rs::cmdline`, `conpty.rs:219` | Argumenty mogą być escaped, lecz program jest dopisywany surowo; `CreateProcessW` dostaje null jako application name | Ścieżka executable ze spacją wymaga regresji i rozwiązania na granicy launchera/dependency. Nie wystarczy `escape_args: true`. |
| `alacritty_terminal-0.26.0/src/tty/windows/mod.rs` | Istnieje ConPTY; publiczny `child_watcher()`, brak unixowego `child()/file()` | Zweryfikować dostęp do uchwytu procesu przed projektem Job Object. Nie wymyślać publicznego API do dołączenia joba. |

## 3. Wspólne zasady implementacji

- Zachować encje AppState i ich właścicieli. Adapter systemowy nie przejmuje
  stanu projektów, tabów ani sesji. I/O i oczekiwanie na procesy są poza UI.
- Dodać małe granice możliwości tam, gdzie są dwaj rzeczywiści odbiorcy:
  katalogi aplikacji, secure credentials, uruchamianie/cleanup procesów.
  Notch i preview mają własne adaptery; nie tworzyć jednego globalnego
  `WindowsManager` ani przebudowywać projektu w dziesiątki crates.
- Nowe uchwyty HWND/HANDLE/COM/hook/subclass mają jednego właściciela RAII,
  opis wątku i kolejności zamykania. Callbacki natywne nie wykonują reentrant
  `Entity::update`; wysyłają ograniczone zdarzenie przez executor GPUI.
- Nie edytować Cargo registry. Jeżeli przypięta zależność wymaga poprawki,
  przygotować minimalny reproducer i kontrolowany patch/upstream lub uzasadnioną
  zmianę wersji. Zachować jedną wersję frameworka i aplikacyjne importy fasady.
- Brak możliwości daje jawny błąd z zachowanymi danymi. Nie zastępować agentów
  shellem, podpisanego commita unsigned ani brakującego playera pustym pane'em.
- Nie zapisywać sekretów, PID, HWND, tokenów run, pipe handles ani bufora PTY
  w SQLite. Nie kopiować profilu użytkownika bez jawnie ustalonego zakresu.

## 4. Zadania dla agentów

### W0 — baza kompilacji i kontrakty platformowe

**Własność:** manifesty, `build.rs`, nowy `src/platform/mod.rs` (jeśli potrzebny),
konfiguracja CI i `docs/windows.md`. Koordynator zmian wspólnych plików.

1. Utrwalić tabelę funkcji z tego audytu w checklistę wykonania, z kolumnami:
   implemented, unit/static, native Windows, native macOS, limitation.
2. Przygotować natywnego runnera MSVC: Rust 1.95.0, Build Tools C++, Windows SDK;
   sprawdzić wymagania cc, vendored OpenSSL/libgit2/libssh2, tree-sitter i rendererów.
   Przypiąć potrzebne narzędzia. Nie rozwiązywać problemów przez usunięcie lockfile.
3. Zinwentaryzować cały graf target Windows i opcjonalne features. Zachować
   `raw-window-handle` w istniejącej wersji; udostępnić go również dla Windows,
   jeśli używany przez adaptery. Win32 bindings dodać w target-specific dependencies
   z minimalnymi features, po sprawdzeniu wersji już rozwiązanych w lockfile.
4. Usunąć bezwarunkowe zależności Unix przez W1, następnie pozostałe błędy
   kompilacji i ostrzeżenia gałęzi Windows. Nie wyłączać całych agentów/testów
   tylko po to, by osiągnąć zielony build.
5. Ustalić jawnie wymagane środowisko dla CLI agentów: natywne EXE, Node launcher,
   zainstalowany Git Bash tam, gdzie provider go wymaga. Nie zakładać obecności WSL.

**Odbiór:** pełny graf i wszystkie cele projektu kompilują się na Windows;
macOS zachowuje zależności i zachowanie. To bramka kompilacji, nie GUI.

### W1 — relay agentów i przenośny protokół hooków

**Własność:** `src/agents/relay.rs`, `launch.rs`, `src/main.rs`,
`src/app_state/agents.rs`, powiązane testy. **Zależność:** uzgodnienia W0.

1. Oddzielić transport od walidacji envelope `{run, token, event}`, rejestru
   tokenów, parsowania eventów i kolejki. Zachować obecny Unix domain socket
   na macOS, włącznie z limitem ścieżki `/tmp`.
2. Windows: preferowany transport to lokalny named pipe. Unikalna nazwa procesu,
   ACL ograniczona do bieżącego użytkownika, odrzucenie klientów zdalnych.
   Sprawdzić Win32 named-pipe API oraz tryb asynchronous/overlapped i anulowanie.
   Nie otwierać przypadkowo serwera TCP na wszystkich interfejsach.
3. Endpoint reprezentować jawnie jako rodzaj + adres; named pipe nie jest
   zwykłą ścieżką pliku. Zachować kompatybilność env helpera albo zmienić
   `Registration`, `environment` i `forward` w jednym kroku.
4. Zachować limity: envelope 1 MiB, kolejka 128, deadline klienta/serwera,
   ACK tylko po zaakceptowaniu eventu. Ograniczyć liczbę klientów i obsłużyć
   klienta, który nie kończy wiadomości. Nie opierać framingu pipe na unixowym
   `shutdown(Write)`; użyć jawnej ramki/długości lub określonego trybu wiadomości.
5. `shutdown` i `Drop` muszą wybudzić accept/read oraz zakończyć worker bez
   bezterminowego connect/join. Late event po unregister nie zmienia sesji.
6. Zweryfikować interpreter hook-command aktualnych CLI Claude/Codex na Windows.
   Zbudować command dla tego interpretera, nie globalny string POSIX. Preferować
   bezpośredni stabilny helper; ścieżki ze spacją/apostrofem/Unicode są obowiązkowe.
7. Rozstrzygnąć helper release: dodanie `windows_subsystem = "windows"` do GUI
   nie może zepsuć stdin/stdout hooka. W razie potrzeby osobny mały
   `canopy-agent-hook.exe`, pakowany razem z GUI, ze wspólnym kodem relay.
8. Zachować trust hooków, prywatne argv/config, konkretne UUID resume,
   nowe run/token przy starcie, brak zapisu `.codex/hooks.json` w repo.

**Testy:** parser i lifecycle z fałszywym transportem; na Windows named-pipe
round trip, obcy/stary token, brak ACK, limit, timeout, równoległy reconnect,
shutdown podczas połączenia. Provider live dopiero osobną kwalifikacją.
**Odbiór:** zdarzenie helpera dociera do właściwego PaneId i zmienia istniejący
model sesji; zamknięcie nie zostawia listenera ani procesu helpera.

### W2 — środowisko, executable, ConPTY i cleanup

**Własność:** `src/terminal/environment.rs`, `session.rs`, `spawn.rs`,
`src/state/tools.rs`, `src/app_state/tools.rs`, `src/app_state/terminals.rs`.
**Zależności:** kontrakty W0; współpraca z W4 przy wspólnym supervisorze.

1. Zdefiniować rodzaj shella i argumenty startowe. Proponowana kolejność dla
   nowej instalacji: skonfigurowany executable → `pwsh.exe` → Windows PowerShell
   → `ComSpec`. Nie dopisywać `-l` do PowerShell/cmd. Nie nadpisywać zapisanych
   własnych argv; migrację starych wartości stosować tylko do jednoznacznie
   rozpoznanego domyślnego profilu, z testem persist/restore.
2. Windowsowy bazowy env pochodzi ze środowiska GUI. Jeśli obsługujemy profile
   PowerShell, wykonać ograniczony probe właściwym interpreterem, z jednoznaczną
   serializacją odporną na banner, Unicode i multiline. Ustalić jawny fallback
   po błędzie profilu; nie wracać do błędu „requires Unix”. Zachować timeout,
   limit wyniku, kolory per proces i brak logowania całego env.
3. Resolve: zmienne bez względu na case (`Path/PATH`), `split_paths`, PATHEXT,
   `.exe/.com` i świadoma obsługa `.cmd/.bat/.ps1`; absolutne ścieżki, UNC,
   drive-relative i separator `\\`. Nie wybierać przypadkowego pliku z cwd.
4. Launch adapter musi odróżniać natywne executable od skryptów/shimów npm.
   Preferować prawdziwy EXE lub Node + osobne argv, gdy znamy format launchera;
   dla skryptu jawnie wskazać interpreter i poprawne escaping. Nie wysyłać
   tekstu komendy do już działającego shella przez PTY.
5. Naprawić/sprawdzić cytowanie **programu** w Alacritty ConPTY. Dodać próbę
   `C:\\Program Files\\...`, a także argumenty puste, spacje, cudzysłowy,
   końcowe backslashe i metaznaki. Nie podwajać escaping argv.
6. Zaprojektować kontrolę drzewa procesu (preferowany Windows Job Object,
   `KILL_ON_JOB_CLOSE`). Sprawdzić publiczne API Alacritty do uzyskania procesu;
   jeśli go brak, potrzebny mały patch dependency. Dołączenie dopiero po spawnie
   ma race z dzieckiem tworzącym potomków: rozwiązać przed deklaracją gwarancji,
   np. atomowy start w jobie lub kontrolowane suspended-create/assign/resume.
7. Zwykły exit, Stop, błąd supervisor spawn, zamknięcie pane/tab/worktree i Quit
   muszą domknąć ConPTY, potomków i czytniki. Usunięcie config/temp dopiero po
   końcu użytkowników uchwytów. Błąd job assignment nie może zostawić ukrytego
   procesu ani uruchomić aplikacji w trybie bez gwarancji cleanup bez informacji.
8. Zachować 64 PTY, lazy start, istniejące buforowanie i drain końcowego outputu,
   exit code Windows, brak sygnału Unix. Nie emulować SIGWINCH; ConPTY resize
   korzysta z istniejącego OnResize. Nie stosować macOS clear adaptera w ciemno.

**Testy:** czyste resolve/argv/env i launch policy; Windows proces testowy
drukujący argv/env oraz tworzący potomka, resize, exit code, końcowy output,
cancel/Stop dwóch tabów, powtórny Stop, restart podczas cleanup, awaria spawnu.
**Odbiór:** shell i agent uruchamiają się z właściwym cwd, a po potwierdzonym
Stop/worktree cleanup nie pozostają procesy objęte zadaniem.

### W3 — katalogi, sekrety i prywatne profile

**Własność:** nowy mały moduł katalogów/credentials, `src/app_state.rs`,
`src/integrations/task_context.rs`, `attachments.rs`, obydwa `credentials.rs`,
`terminal/agent_config.rs`, copy Preferences. **Zależności:** W0, kontrakt env W2.

1. Jedna funkcja katalogu danych: jawny `CANOPY_DATA_DIR` ma pierwszeństwo;
   macOS zachowuje obecną ścieżkę, Windows używa Known Folder LocalAppData +
   `Canopy Rust`. Baza i task-context muszą używać tej samej decyzji.
   Osobno user home do `.ssh`/profili; nie podmieniać globalnego HOME.
2. Utworzenie katalogów i błędy uprawnień mają istniejącą drogę błędu UI.
   Zachować dane i zapisane niedostępne ścieżki; import macOS/Electron nie
   przekłada samowolnie cwd na inny dysk ani nie resetuje workspace.
3. Implementować Windows Credential Manager (`CredReadW`, `CredWriteW`,
   `CredDeleteW`, `CredFree`) lub równoważny sprawdzony OS backend. Zachować
   osobne namespaces usług i stabilne ID; SQLite zawiera wyłącznie referencje.
   Operacje blokujące na workerze, poprawne UTF-8/UTF-16 i limity payloadu.
4. Błąd remove musi być widoczny i nie udawać usunięcia sekretu. Empty field
   zachowuje wpis; Delete usuwa tylko wybrane ID. Brak plaintext fallbacku.
   W UI nazwa magazynu zależy od platformy; maskowane custom env nadal SQLite.
5. Katalogi prywatnych configów, preview i task context: sprawdzić odziedziczone
   ACL i w razie potrzeby nadać DACL bieżącego użytkownika; uwzględnić cleanup
   read-only plików i sharing violations. Nie wymagać administratora.
6. Gemini: dobrać sposób zachowania istniejącej konfiguracji/OAuth bez wymagania
   Developer Mode dla symlinków. Preferować mechanizm override wspierany przez
   CLI; jeśli potrzebna prywatna kopia, ustalić allowlist, uprawnienia i politykę
   odświeżanych poświadczeń. Nie kopiować wszystkiego ani nie gubić auth silently.
7. Uodpornić `preview_name` na nazwy zastrzeżone Windows (CON/NUL/COM1 itd.),
   trailing dot/space i niedozwolone znaki; zachować rozszerzenie i limit.

**Testy:** katalogi bez HOME, env override, brak dostępu; credentials na
unikalnych testowych ID (nigdy wpisach użytkownika); store/load/update/remove,
brak wpisu, błąd, Unicode i limit. Profile: dwie równoległe sesje z różnymi
JSON, oryginał nietknięty, auth dostępne i cleanup po zamknięciu.
**Odbiór:** restart odtwarza dane i referencje, integracje pobierają tokeny,
nie powstaje `Library/Application Support` na Windows.

### W4 — Git hooks, podpisy i uwierzytelnianie

**Własność:** `src/git/hooks.rs`, `signing.rs`, `network.rs`,
`ssh_credentials.rs`; ewentualny wspólny bounded process runner z W2.
**Zależności:** W2, W3; Git pozostaje na jednym workerze libgit2.

1. Windows runner musi równolegle drenować stdout/stderr z limitem pamięci
   oraz deadline. Zakończyć potomków przed join czytników; uwzględnić pipe
   zatrzymany przez potomka po zakończeniu rodzica. Usunąć Unix-only pipe error.
2. Ustalić obsługę executable/shebang hooków zgodną z używanym Git for Windows:
   znaleźć odpowiedni interpreter bez założenia `/bin/sh`, zachować cwd,
   `core.hooksPath`, args i oczyszczony env. Brak interpretera = błąd z draftem,
   nie pominięcie obowiązkowego hooka. `GIT_EDITOR=:` wymaga kwalifikacji.
3. Zachować pre-commit → prepare-commit-msg → commit-msg → podpis → ponowną
   walidację HEAD/index → publikację → post-commit. Post failure jest warningiem
   istniejącego commita; merge-before-remove zachowuje te same zabezpieczenia.
4. Podpisy GPG/SSH: resolve `.exe`, program ze spacją, klucz `~/`, gpg-agent,
   pinentry/SSH_ASKPASS i cancel. Bez okna konsoli dla pomocniczych procesów,
   ale nie ukrywać potrzebnego pinentry. Brak unsigned fallbacku.
5. HTTPS: określić schemat lookup wpisu Credential Manager na podstawie
   sparsowanego URL (scheme, host, port, username; ścieżka zgodnie z polityką).
   Nie obiecywać automatycznej zgodności z każdym wpisem Git Credential Manager.
   Jeśli czytamy GCM-compatible wpisy, udokumentować i przetestować dokładny
   format; dla braku wpisu zapewnić konkretną drogę konfiguracji poświadczenia.
   Nie używać shellowego credential helpera ani tokenów trackerów do innych hostów.
6. SSH: sprawdzić libssh2 OpenSSH-agent na Windows, fallback plików i znane
   host keys. Przy luce zaplanować zgodny adapter/patch, nie globalny bypass
   weryfikacji ani przejście na subprocess Git. Zachować bounded attempts.

**Testy:** lokalne repo/remote; hook zmieniający index/message, odrzucenie,
brak interpretera, timeout, potomek; podpis poprawny/odrzucony/anulowany;
lokalny serwer Git dla auth/ref rejection/timeout. Realny remote tylko w
uzgodnionym zakresie. **Odbiór:** potwierdzony rezultat operacji, zachowany draft
po błędzie i nieblokujący quit; transport OK nie jest utożsamiany z accepted push.

### W5 — natywny notch Windows

**Własność:** `src/ui/notch.rs`, `notch_motion.rs`, `notch_macos.rs`, nowy
`notch_windows.rs` i wspólna geometria; `components/notch.rs`, gates w
`ui/mod.rs`, `components/mod.rs`, `theme.rs`, `src/app.rs`. **Zależności:** W0;
pełny odbiór wymaga W1/W2 i motion z W6.

#### W5a. Wspólny kontroler

1. Przenieść `InputRegion::contains` do modułu niezależnego od AppKit.
   `Geometry`, `Motion`, filtry i projekcja sesji zostają wspólne.
2. Wąski adapter oferuje instalację na żywym oknie, aktualizację regionu,
   zmiany monitora/DPI, zdarzenie inside/outside i cleanup. Uchwyty przechowuje
   encja Notch; adapter nie tworzy drugiej listy sesji ani timerów notyfikacji.
3. Odsłonić komponenty/motion/token i start dla macOS + Windows; inne systemy
   zachowują jawny unsupported. `CANOPY_NOTCH_PREVIEW` również Windows.
4. Zachować reguły unseen, aktywnego okna i wszystkich pane'ów splitu,
   Needs attention, auto-notice 4 s, hover overview i kierowanie do PaneId.
   Błąd instalacji nie może pozostawić transparentnego okna blokującego pulpit:
   schować/zamknąć overlay, pozostawić główne okno i widoczny błąd.

#### W5b. Okno Win32 i brak aktywacji

1. Uzyskać HWND przez sprawdzony `HasWindowHandle` / Win32 raw handle na UI.
   Zachować GPUI Root i transparentne okno o stałej maksymalnej ramce.
2. Sprawdzić `WS_EX_NOACTIVATE`, istniejące TOOLWINDOW/TOPMOST,
   `SWP_NOACTIVATE` i `WM_MOUSEACTIVATE -> MA_NOACTIVATE` wyłącznie dla notcha.
   Hover/scroll/filter/automatyczna notyfikacja nie zmieniają foreground window.
3. Subclass procedury okna musi współistnieć z procedurą GPUI, delegować wszystkie
   pozostałe komunikaty, obsłużyć WM_NCDESTROY i bezpiecznie usunąć callback.
   Preferować scoped subclass API; nigdy globalny patch zachowania wszystkich okien.
4. Tylko kliknięcie sesji jawnie przywraca/aktywuje główne okno i wybiera workspace,
   tab oraz pane. Sprawdzić zminimalizowane okno i ograniczenia foreground Windows.
   Overlay nie pojawia się jako osobny przycisk taskbara/Alt+Tab.

#### W5c. Click-through i animacja

1. **Nie uznać samego `HTTRANSPARENT` za rozwiązanie**: jego forwarding ma
   ograniczenia między wątkami; wymagana próba nad oknem innego procesu.
   Sama alfa ani `WS_EX_TRANSPARENT` również nie jest wystarczającym dowodem.
2. W pierwszym spike porównać region okna/input region zgodny z DirectComposition
   (np. `SetWindowRgn`) z kontrolowanym przełączaniem ignorowania inputu.
   Wybrać metodę po dowodzie braku przechwytywania kliknięć poza wyspą i
   poprawnego renderowania zaokrągleń. Nie zmieniać ramki HWND w animacji.
3. Region ma odpowiadać bieżącej, nie docelowej geometrii, z pustym regionem
   przy braku sesji. Przeliczać go przy postępie motion, zmianie content i DPI;
   uwzględnić nieruchomy kursor podczas zwijania/rozszerzania.
4. Wykrycie wejścia musi działać także przy przepuszczaniu inputu. Jeśli potrzeba
   monitorowania globalnego, porównać scoped mouse hook/raw input z wymaganiami
   backendu. Callback tylko sygnalizuje zmianę, nie blokuje systemowej kolejki,
   nie zapisuje historii kursora i nie przechwytuje klawiatury.
5. Testować przejście z label do ikony, szybki hover out/in, rogi, scroll,
   zmianę filtra i nowe powiadomienie w trakcie zamykania. Bez idle polling
   i bez pętli notify. Mouse capture zwolnić przy destroy/ukryciu.

#### W5d. Monitory i DPI

1. Początkowy monitor: primary jak dziś. Pełne granice monitora i work area
   rozróżniać jawnie. Domyślnie górny środek; jeśli taskbar zajmuje tę krawędź,
   zakotwiczyć do górnej krawędzi dostępnego obszaru, bez zasłaniania taskbara.
2. Piksele GPUI są logiczne, Win32 często fizyczne: jedna jawna konwersja,
   ujemne originy i skale 100/125/150/200%. Ramkę można przeliczyć przy zmianie
   monitora/DPI, nie w każdej klatce animacji. Clamping dla małych ekranów.
3. Obsłużyć WM_DPICHANGED/WM_DISPLAYCHANGE, zmianę primary, odłączenie ekranu,
   taskbar auto-hide i resume po blokadzie/uśpieniu. Subskrypcje mają cleanup.
4. Proponowana polityka: overlay na bieżącym virtual desktop, bez narzucania
   przypięcia do wszystkich desktopów; schowany nad fullscreen aplikacji obcej.
   Zapisać rzeczywisty zakres kwalifikacji, nie obiecywać secure desktop/UAC.

**Testy jednostkowe:** region z rogami, DPI/ujemne współrzędne, clamping,
retarget, filtry/notice, zniknięcie ostatniej sesji, lifecycle błędu adaptera.
**Odbiór native:** prawdziwa mysz nad przeglądarką/innym procesem, brak utraty
focusu i blokowania tabów, właściwe aktywowanie pane'a, brak czarnych prostokątów,
brak pozostawionych hooks/HWND po quit. Nagranie i identyfikacja uruchomionego SHA.

### W6 — okna, skróty, fonty i dostępność

**Własność:** `src/app.rs`, `src/ui/components/titlebar.rs`, `src/ui/mod.rs`,
Preferences, `src/ui/terminal/{mod,input}.rs`, `src/ui/theme.rs`, `src/motion/mod.rs`.
**Zależność:** W0; uzgadniać wspólne pliki z W5.

1. Wspólne komendy używają `secondary-*` albo jawnej tabeli per platforma.
   Terminal: proponowane Ctrl+Shift+C/V, zachowane Ctrl+C/D/Z i Tab/Shift+Tab
   do PTY. Audyt konfliktów wszystkich globalnych Ctrl ze sterowaniem CLI,
   np. Ctrl+W/P/D; jeśli potrzeba, warianty z Shift w kontekście Terminal.
   Nie robić mechanicznej zamiany wszystkich `cmd` na `ctrl`.
2. Tekst znakowy/IME ma pierwszeństwo nad kodowaniem Ctrl+Alt dla AltGr.
   Sprawdzić polskie ą/ę/ł/ó/ż/ź, dead keys, composition, selection i schowek
   zarówno w editor/input, jak i terminalu. UI pokazuje Windows shortcut labels.
3. Windows caption buttons minimalizuj/maksymalizuj/przywróć/zamknij mają być
   dostępne w głównym oknie, Preferences i preview. Preferować komponent TitleBar
   lub sprawdzone `WindowControlArea`; zachować product controls i prawa sidebara.
   Usunąć tylko macOS-specific traffic-light gap na Windows.
4. Sprawdzić drag, dwuklik, Snap Layouts, resize corners, maximized bounds,
   Alt+F4 oraz oddzielne okno Preferences. Close głównego okna prowadzi przez
   dirty guards, final save i cleanup; sam notch nie utrzymuje aplikacji.
5. Font UI wybrać platformowo (Windows systemowy/Segoe UI według faktycznego API),
   zachować terminal Mono i glyph grid. Nerd fallback dystrybuować tylko z
   właściwą licencją; nie kopiować fontów macOS. DPI nie zmienia liczby kolumn
   przez przypadkowe skalowanie całego UI.
6. Podłączyć Windows ustawienie animacji, np. sprawdzone
   `SPI_GETCLIENTAREAANIMATION` + WM_SETTINGCHANGE lub odpowiednik frameworka,
   do istniejącej polityki. Cache/subskrypcja, bez systemowego I/O w renderze.
   Wyłączenie animacji w trakcie przejścia kończy je poprawnie.
7. Nonactivating notch ma klawiaturową alternatywę w głównym oknie (lista sesji
   i nawigacja do konkretnego pane'a). Nie przejmować globalnie klawiatury dla
   samego hoveru. Sprawdzić focus i nazwy kontrolek w accessibility tree Windows.

**Odbiór:** główne workflow są dostępne myszą i klawiaturą, wpisywanie do
terminala nie wywołuje omyłkowo operacji workspace, ustawienia motion działają.

### W7 — filesystem, watchery i bezpieczne operacje worktree

**Własność:** `src/files*`, `src/git/*watch*`, `worktree_identity.rs`,
`removal.rs`, `worktree_workflow.rs`, `src/state/projects.rs`,
`src/terminal/file_drop.rs`, formatter referencji `task_context.rs`.
**Zależności:** W2 (dialekt launchera), W3 (katalogi).

1. Zdefiniować tożsamość ścieżek dla drive/UNC/extended prefix (`\\?\\`),
   Unicode i katalogów z case-sensitive flag. Nie lowercasować globalnie
   wszystkich ścieżek; rozróżniać ścieżkę prezentowaną, Git-relative i systemową.
2. Utrzymać granice root: junction/symlink/reparse point nie może ominąć
   zabezpieczenia edycji/usuwania. Missing to rzeczywisty NotFound, nie AccessDenied
   ani sharing violation. Kanonizować istniejącego rodzica brakującego worktree.
3. Sprawdzić atomic save/persist na Windows: open handles, antivirus,
   read-only, ACL, BOM/CRLF i konflikt. Błąd pozostawia dirty buffer i oryginał;
   nie dodawać delete-before-rename. Dokumentować granicę wyścigu z obcym writerem.
4. Watchery: otwarcie/zwinięcie katalogu, nowe pliki, save-by-rename, zmiana
   tracked w zamkniętym folderze, ignorowane katalogi, common Git dir i linked
   worktree. Overflow/błąd zdarzeń musi wywołać ograniczony rescan/ostrzeżenie;
   brak okresowego skanowania w spoczynku. Zwalniać watchery przed cleanup katalogu.
5. Usuwanie worktree po Stop musi czekać również na uchwyty mediów/config/watch;
   ponownie walidować fingerprint/status/ref/OID i lock. Sharing violation
   pozostawia czytelny retry/partial success; nie używać szerokiego force delete.
6. Drop do shella formatować według jego dialektu, do prompta agenta według
   gramatyki referencji providera. Zachować bracketed paste, limit, odrzucenie
   znaków sterujących i **brak Enter**. `shell_words::split` w Preferences jest
   formatem listy argv: zdecydować o zgodnym formacie edycji/persist, aby nie
   gubić backslashy Windows i nie reinterpretować zapisanych argumentów.

**Testy:** dysk NTFS, spacje/Unicode, junction, read-only, UNC jeśli deklarowany,
brakujący katalog vs brak dostępu, zmiana HEAD podczas potwierdzenia, otwarty
uchwyt blokujący usunięcie, watcher po switch/quit. Destrukcyjne próby wyłącznie
na disposable repo. **Odbiór:** żaden Windows-specific błąd nie obchodzi
istniejącego mechanizmu potwierdzeń i nie usuwa workspace po nieudanym cleanup.

### W8 — video i podgląd załączników

**Własność:** `src/ui/video_*`, `attachment_*`, `src/app_state/editors.rs`,
`src/ui/workspace_panes.rs`, `build.rs`, nowe adaptery Windows. **Zależności:**
W3 (pliki prywatne), W6 (okna/focus), W7 (cleanup).

1. Najpierw zapewnić jawny stan unsupported/error dla `PaneKind::Video` na
   Windows; nie zostawiać pustego widoku. To etap przejściowy, nie końcowa parytet.
2. Video: preferowany spike Media Foundation z renderowaniem do child HWND
   albo tekstury zgodnej z GPUI. Zweryfikować COM apartment, message loop,
   dostępne API i backend renderera, zanim powstanie nowy publiczny interfejs.
3. Wspólny kontrakt playera: load, ready/error, paused/playing, duration/time,
   seek, visibility/occlusion, release. Pierwszy start i restore paused;
   tab switch/modal zatrzymuje odtwarzanie i ukrywa natywną powierzchnię.
4. Przetestować native-child overlay ordering: żaden obraz nad modalem,
   tooltipem ani sąsiednim pane'em; focus wraca do właściwej kontrolki.
   Nie uznawać Windows child HWND za automatyczny odpowiednik CALayer.
5. Kodeki: kwalifikować konkretne MP4/MOV/M4V; MKV zależnie od systemowych
   kodeków z jawnym błędem. Bez obietnicy odtwarzania każdego rozszerzenia,
   bez nieuzgodnionego pakowania FFmpeg/codecs.
6. Quick Look nie ma bezpośredniego powszechnego odpowiednika. Router preview
   powinien używać istniejących image/text/font i nowego video. Dla PDF/Office
   sprawdzić Windows Preview Handlers (`IPreviewHandler`) i dostępność handlerów
   na czystym systemie. Preferować izolowany host helpera dla cudzych handlerów,
   z timeout/cleanup; nie ładować dowolnego shell extension bez oceny lifecycle.
7. Dla formatu bez handlera pokazać nazwę, typ, rozmiar i Save; ewentualne
   „Open in default app” wyłącznie po akcji użytkownika i ze świadomym lifecycle
   temp pliku. Nie auto-uruchamiać pobranego załącznika/skryptu.
8. Zachować read-only preview, auth/redirect rules pobierania, limity i cleanup.
   Zamknięcie podczas download/decode nie może zamontować spóźnionego widoku.

**Odbiór:** video play/pause/seek/restore i bezpieczny podgląd obsługiwanych
załączników działają; formaty zależne od opcjonalnego handlera opisane w
macierzy. Zewnętrzna aplikacja nie jest dowodem wewnętrznego playera.

### W9 — pakowanie, CI i końcowa kwalifikacja

**Własność:** nowe `scripts/build-windows.ps1`, packaging assets/manifest,
CI, `docs/windows.md`, aktualizacje dokumentacji domenowej. **Zależność:** W0–W8.

1. Powtarzalny release MSVC z `--locked`, bez inspektora/HUD. EXE ma ikonę,
   wersję, manifest DPI/long paths stosowny do backendu; sprawdzić istniejącą
   inicjalizację GPUI, aby nie ustawiać sprzecznej DPI awareness drugi raz.
2. GUI bez konsoli, helper hooka z działającym stdin i bez migających okien.
   Dystrybuować helpery, wymagane runtime DLL oraz dozwolone zasoby/licencje.
   Działać z cwd innym niż repo i ścieżki instalacji zawierającej spacje/Unicode.
3. Pierwszy artefakt ZIP do kwalifikacji; następnie wybrać jeden installer
   per-user, np. WiX/MSI lub MSIX po sprawdzeniu full-trust/PTY/hooks/preview.
   Nie budować kilku formatów bez potrzeby. Update nie nadpisuje uruchomionego
   helpera, uninstall nie usuwa domyślnie danych użytkownika.
4. CI: natywny Windows i macOS, fmt/Clippy, jawne zestawy unit/platform tests,
   build/package po uzyskaniu właściwej zgody na wykonanie. Unix-only test
   oznaczyć platformowo tylko wtedy, gdy istnieje Windowsowy odpowiednik lub
   jawnie udokumentowana luka. Nie zakrywać funkcji przez `#[ignore]`.
5. Dokumentacja: `docs/notch.md` zawiera historyczny opis mocka sprzeczny z
   obecnymi sesjami; przy implementacji zastąpić go aktualnym kontraktem.
   Zaktualizować także terminal, agents, tools, settings, integrations,
   git-network, git-hooks, files-editor, verification oraz platformowe wpisy
   AGENTS.md, tylko w zakresie faktycznie dostarczonym i sprawdzonym.

**Odbiór:** instalacja/uruchomienie/aktualizacja/odinstalowanie na czystym
Windows, zachowany katalog danych, właściwy SHA uruchomionego procesu,
pełna macierz dowodów i lista pozostałych ograniczeń. Podpisanie i publikacja
wydania są oddzielnymi krokami operacyjnymi.

## 5. Kolejność i przekazanie pracy

1. W0 ustala środowisko i granice; W1 usuwa twardą blokadę Unix.
2. W2 + W3 zapewniają uruchamianie i dane; potem W4 domyka Git.
3. W5 można prowadzić od spike okna po W0, ale jego odbiór na realnych sesjach
   wymaga W1/W2. W6 powinno poprzedzić końcowy odbiór W5.
4. W7 kwalifikuje filesystem; W8 podglądy; W9 zamyka całość.

Jeżeli wykonanie zostanie rozdzielone między agentów, przed pracą przypisać
wyłącznego edytora wspólnych `Cargo.toml`, `Cargo.lock`, `app.rs`, `ui/mod.rs`,
`app_state.rs` i `theme.rs`. Inni przekazują potrzebną zmianę koordynatorowi.
Nie revertować cudzych zmian. Małe handoffy mają zawierać:

- SHA bazowe, zmienione pliki i kontrakt adaptera;
- implementację oraz konkretne scenariusze testów z wynikami;
- dowód native Windows/macOS lub jawne „niewykonane”;
- znane ograniczenia, potrzebne zależności i kolejny warunek odbioru.

Nie oznaczać W5 jako ukończonego na podstawie sztucznych sesji. Taki spike
potwierdza wyłącznie okno/input. Model zdarzeń i powiązanie run → pane sprawdzić
oddzielnie, a całość na rzeczywistym providerze w uzgodnionym środowisku.

## 6. Macierz końcowej weryfikacji

W tej sesji wykonano tylko audyt źródeł i dokumentację. Poniższe sprawdzenia
są **planowane**, nie wykonane. Aktualne instrukcje dopuszczają po implementacji
unit tests, lint i formatowanie. Build/package oraz testy procesu, integracyjne
i GUI wykraczające poza tę zgodę należy uruchamiać dopiero w autoryzowanym
zakresie; E2E wymaga wcześniejszego pytania i otrzymania zgody. Zakończyć
wcześniej wszystkie niezależne, dozwolone prace.

| Warstwa | Minimalny dowód |
| --- | --- |
| Format/lint | `cargo fmt --all -- --check`; `cargo clippy --locked --all-targets -- -D warnings` na obu platformach po naprawie właściwych gałęzi |
| Unit | Czysta geometria/DPI, motion, parser relay, stany cleanup, argv/env, ścieżki, credential mapping, routing preview; selektywnie uruchomione zestawy |
| Build | Natywny release MSVC i macOS, osobno test feature `dev-inspector`/`frame-profile` jeśli zmieniony zakres tego wymaga |
| Platform/process | Named pipes, ACL/Credential Manager na testowych ID, ConPTY/process tree, Git hook/signing timeouts, watchery i atomic save |
| Okna/input | Start/close/Alt+F4, Preferences, Snap, DPI, restore, Ctrl shortcuts, AltGr, IME, clipboard i drop bez Enter |
| Notch | Prawdziwa mysz, klik w obcy proces poza wyspą, brak focus steal, animacja z nieruchomym kursorem, klik sesji, monitory, fullscreen/virtual desktop według deklaracji |
| Sesje | Claude/Codex Start → Working → Needs attention → Completed/Failed, seen/unseen, subagent podczas oczekiwania, restart/resume konkretnego UUID |
| Git/dane | Czysty i brudny worktree, stop-before-delete, drugie potwierdzenie, lock/OID race, remote rejection, podpis, dirty editor i błąd final save |
| Media | Pierwsza klatka, play/seek/pause, tab switch/modal, restore paused, close podczas decode i brak blokujących uchwytów |
| Dystrybucja | Czyste konto bez toolchaina/repo w cwd, ścieżka ze spacją, instalacja/update/uninstall bez utraty danych i brakujących DLL |
| Regresja macOS | Obecne AppKit notch, SIGWINCH, Keychain, AVFoundation/Quick Look, skróty, worktree i quit pozostają poprawne |

Brak dostępu do fizycznego Windows/GPU pozostawia tę warstwę niezweryfikowaną.
Headless ani cross-compilation nie dowodzą click-through, focusu, kodeków lub
płynności. Nie deklarować 120 FPS bez osobnych pomiarów release/profiling.

## 7. Najważniejsze decyzje wymagające dowodu przed zamknięciem portu

1. Notch: wybrana metoda regionu/input działa nad innymi procesami i z GPUI
   DirectComposition; nie przejmuje aktywacji.
2. ConPTY: poprawne cytowanie executable i kontrola potomków bez race.
3. Provider hooks: rzeczywisty interpreter Windows i stabilny helper release.
4. SSH: rzeczywista zgodność libssh2 z dostępnym agentem Windows.
5. Gemini: overlay zachowuje auth bez wymogu admin/symlink i bez niejawnego
   nadpisywania oryginału.
6. Preview: lista gwarantowanych formatów oraz opcjonalnych systemowych handlerów.

To są bramki implementacyjne dla następnych agentów, nie powód do zastąpienia
funkcji mockiem lub zadeklarowania pełnego wsparcia po samym buildzie.
