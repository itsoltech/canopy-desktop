# Sesje agentów, hooki i resume

`AppState.agents` jest oddzielną encją runtime. Integracja obejmuje Claude Code
oraz Codexa uruchamianych bezpośrednio w PTY. Każdy start dostaje nowy run UUID
i capability token; trwałe ID sesji dostawcy jest zapisane w PaneMetadata.resume_id.
PID, token, adres socketu ani status pracy nie trafiają do SQLite.

## Uruchamianie i konfiguracja

Jeden prywatny Unix socket na Unix albo chroniony named pipe na Windows odbiera
hooki wszystkich pane'ów. Ramka ma maksymalnie 1 MiB, cała wymiana ma deadline,
a kolejka jest ograniczona. Helper tylko przekazuje JSON ze stdin do odbiornika.
Nie otwiera GUI i nie wypisuje ACK ani kontekstu na stdout providera. Token i run
UUID są przekazywane przez środowisko procesu.

Konfiguracja powstaje na prywatnej kopii wybranego profilu:
- Claude: dodatkowe hooki w --settings JSON.
- Codex 0.154.0: inline hooks przez --config oraz --enable hooks. Na Windows
  `commandWindows` uruchamia konsolowy helper przez jawny, kodowany UTF-16LE
  skrypt PowerShell, dzięki czemu działa zarówno z domyślnym `cmd.exe /C`, jak
  i wtedy, gdy shell hooków jest PowerShellem. Przed startem Canopy sprawdza
  obecność i wersję helpera obok uruchomionego EXE. Windowsowy Codex ma 3 s
  na start PowerShella, helpera i zakończenie transportu; pozostałe hooki
  zachowują limit 2 s. Jest to maksymalny limit akceptowany przez Codex 0.154.0,
  więc konfiguracja nie generuje ostrzeżeń o clampowaniu.

Istniejące hooki profilu są zachowane, a pliki projektu i konfiguracji użytkownika
nie są przepisywane. Definicja polecenia pomocniczego nie zawiera per-run tokenów
ani zmiennych ścieżek i pozostaje stabilna między startami tej samej instalacji.
Codex może wymagać zatwierdzenia Canopy helper w /hooks. Nie wyłączamy mechanizmu
zaufania ani reguł zarządzanych. Przeniesienie aplikacji zmienia ścieżkę helpera
i może wymagać ponownego zatwierdzenia.

Źródła: [Codex hooks](https://developers.openai.com/codex/hooks),
[źródło zainstalowanej wersji](https://github.com/openai/codex/tree/rust-v0.154.0/codex-rs/hooks).

Kontrakt Stop w Codex 0.154.0 uznaje kod wyjścia 0 i pusty stdout za poprawne.
Niepusty stdout jest interpretowany jako JSON odpowiedzi Stop, dlatego helper
pozostaje całkowicie cichy. Claude zachowuje własną definicję `command`; nie
dostaje `commandWindows` ani formatu odpowiedzi właściwego tylko dla Codexa.

## Tożsamość i trwałość

Hook jest wiązany z pane'em przez rejestrację uruchomienia, nigdy tylko po cwd.
ID dostawcy zapisujemy po pierwszym poprawnym zdarzeniu; istniejący writer scala
zapis do SQLite. Nie czekamy z rejestracją ID do quit. Zapis sprawdza PaneId,
tool, profil, cwd i oczekiwane poprzednie ID; nie nadpisuje nowej tożsamości
spóźnionym wynikiem. Przeniesienie pane'a między tabami zachowuje jego ID i sesję.

Przy restarcie przygotowanie launch czyta aktualne metadane pane'a, zamiast
starej kopii TerminalView. Claude otrzymuje --resume UUID, Codex resume UUID.
Nie używamy --last/pickera ani nie uruchamiamy nowej sesji po błędzie wznowienia.
UUID jest walidowane, a sprzeczne własne argumenty resume powodują błąd.
W zakończonym pane'ie Restart ponawia sesję, a jawne New session usuwa tylko
wiązanie w pane'ie i uruchamia nową sesję (nie kasuje historii dostawcy).

Nowe uruchomienie unieważnia stary token. Zamknięcie procesu/pane'a wyrejestrowuje
hooki; zdarzenia zakończonego uruchomienia są odrzucane. Nowy SessionStart w już
potwierdzonym uruchomieniu może przestawić pane na nową sesję użytkownika;
poprzednie identyfikatory tego uruchomienia są pamiętane jako nieaktualne.

Dotychczasowy lazy lifecycle pozostaje: restore startuje tylko pane'y aktywnego
taba wybranego workspace'u. Pozostałe taby wznowią swoje konkretne sesje przy
pierwszym wyborze. Błąd końcowego zapisu nie wyłącza odbiornika hooków.

## Inspektor i notch

Inspektor pokazuje wybrany pane: stan, model, tryb, profil, ID sesji, ostatnie
narzędzie, ostatnią odpowiedź (gdy hook ją dostarcza) i ograniczoną historię zdarzeń.
Brak hooków pozostaje Starting/Waiting for events, nie jest zgadywanym Idle.
Koszt i zużycie kontekstu nie są obecnie pozyskiwane ani fabrykowane.

Notch agreguje uruchomione sesje, także z nieaktywnych tabów/worktree.
Najpierw pokazuje wymagające uwagi, potem błędy/pracę. Lista ma do pięciu
widocznych wierszy i scroll, a kliknięcie wybiera właściwy workspace, tab i pane.
Natywna ramka notcha pozostaje stała; zmienia się tylko widoczna wyspa i jej
region wejścia. Zachowano przepuszczanie kliknięć przez przezroczysty obszar.

Session Inspector jest klawiaturową alternatywą dla nieaktywującego notcha.
Na początku treści pokazuje aktywne lub nieobejrzane sesje jako dostępne
klawiaturą przyciski ze stabilnym PaneId. Cmd+Shift+S na macOS albo
Ctrl+Shift+A na Windows otwiera stronę Session; wybór wiersza prowadzi przez tę
samą akcję focus do właściwego workspace, taba i pane'a.

PreToolUse(request_user_input/AskUserQuestion) i PermissionRequest oznaczają
oczekiwanie. Tool-use ID wiąże odpowiedź z pytaniem; niezwiązane PostToolUse nie
kasuje oczekiwania. Po odpowiedzi i zakończeniu tury status wraca Working/Idle.
Wykrywanie korzysta z hooków, nie z parsowania znaków terminala ani pollingów
transkryptu. Treść pytania jest ograniczona i pokazywana w notchu/inspektorze.

## Weryfikacja i ograniczenia

- Testy: routing i tokeny, wyrejestrowanie, zachowanie hooków użytkownika,
  wersję i ciszę pakowanego helpera, argv resume obu dostawców, SQLite + lazy
  activation, odrzucenie starego wiązania, pytania i niezwiązane zakończenia
  narzędzi. Test natywny Windows wykonuje wygenerowane `commandWindows` przez
  domyślne `cmd.exe /C` Codexa i skonfigurowany PowerShell, wysyła Stop i wymaga
  pustego stdout/stderr oraz zdarzenia relay.
- GUI: dwa Codexy w jednym katalogu z odrębnymi ID; po restarcie porównano
  mapę PaneId → session ID i odtworzoną historię obu pane'ów. Nowa tura po resume
  dostarczyła odpowiedź do inspektora. Pytanie request_user_input ustawiło
  Needs attention, odpowiedź PostToolUse i Stop przywróciły Idle.
- Claude 2.1.266: test w Canopy ze środowiskiem login/interactive fish potwierdził
  uruchomienie, AskUserQuestion → Needs attention oraz Esc → Idle z usunięciem
  pytania w inspektorze. Wcześniejsza próba bez środowiska shella nie miała autoryzacji.
- Headless codex exec wykonał turę, lecz próba diagnostyczna nie dostarczyła
  hooków do testowego helpera. Produkt używa TUI w PTY; ten wariant został
  zweryfikowany oddzielnie w GUI. Ignorowane testy CLI wymagają właściwego
  logowania oraz zatwierdzenia używanej ścieżki helpera.
- Ograniczenia: nie ma zdalnego zatwierdzania uprawnień z inspektora, statusline
  kosztów/tokenów ani pełnego transkryptu. Niedostępne/niezatwierdzone hooki
  nie mogą zapewnić informacji, których dostawca nie wysyła.
- Nowa ścieżka `commandWindows` i zgodność z żywym Codexem/PowerShellem nadal
  wymagają ponownego uruchomienia na Windows; testy macOS tego nie kwalifikują.

## Widoczność i powiadomienia notcha

Agent jest traktowany jako widoczny, gdy główne okno ma fokus, wybrany jest jego
workspace, a pane należy do aktywnego taba. Wszystkie pane'y splitu są widoczne,
nie tylko pane z fokusem klawiatury. Aktywność okna pochodzi z obserwatora GPUI,
a zmiany tabów/worktree z modelu, bez pollingu.

Dla niewidocznego agenta wejście w Needs attention/Error oraz zakończenie pracy
powoduje krótkie automatyczne rozwinięcie notcha (4 s, bez przejmowania focusu).
Powtarzające się Working, start sesji i jawne Stop/Close nie tworzą powiadomień.
Nieobejrzany wynik zostaje oznaczony do czasu pokazania pane'a w aktywnym oknie;
po zakończeniu procesu pozostaje dostępny w notchu do obejrzenia. Kliknięcie
notcha przełącza workspace/tab/pane, a obserwator widoczności potwierdza odczyt.
Ręczny hover nadal działa niezależnie od automatycznego rozwinięcia.

Widoczność jest regułą aktywne okno + aktywny tab, nie analizą pikselowego
zasłonięcia okna przez inne powierzchnie. Testy obejmują wszystkie pane'y splitu,
inny tab/workspace, nieaktywne okno i filtr istotnych przejść.

Kliknięcie wiersza notcha wybiera workspace zawierający aktualny PaneId,
a następnie jego tab i pane. Przy zmianie worktree cel ustawiamy w zapisanym
modelu przed publikacją, aby lazy start nie uruchamiał poprzednio aktywnego taba.
Ponowne kliknięcie już wybranego pane'a przywraca również fokus terminala.

Automatyczne rozwinięcie pokazuje wyłącznie sesje z nieobejrzaną zmianą statusu
(`unseen`). Najechanie na zwinięty notch otwiera pełny podgląd, z filtrami All, Idle,
Working, Needs attention i Error. Starting należy do Working, a zakończony proces
oczekujący na obejrzenie — do Idle. Filtr podglądu nie ukrywa powiadomień.

Viewport obejmuje maksymalnie 8 wierszy; pozostałe sesje są dostępne przez scroll.
Filtry pozostają nad przewijaną listą. Zmiana filtra lub trybu zeruje scroll;
pusta kategoria pokazuje komunikat bez usuwania filtrów. Natywna ramka pozostaje
stała, a wysokość widocznej wyspy dostosowuje się istniejącą animacją RESIZE.

Claude cancellation: subscribe to PermissionDenied and StopFailure, preserve
PostToolUseFailure.is_interrupt, and clear both the tool-use ID and the id-less
PermissionRequest entry on matching completion. Declined AskUserQuestion and
explicit interruption map to Idle; ordinary tool failures remain Error.
Typed idle_prompt/agent_completed notifications clear stale attention;
unrelated notifications (e.g. auth_success) do not create an attention state.
Reference: https://code.claude.com/docs/en/hooks .

Live Claude 2.1.266 replay confirmed that Esc on AskUserQuestion emits neither
Stop nor a tool-completion hook. `agents/transcript.rs` therefore watches the
hook-provided session transcript only while that question is pending. It matches
sessionId + tool_use_id + an error tool_result with Claude's explicit rejection
marker, never terminal text or the Escape key itself. Reads are off the UI thread,
bounded to the last 2 MiB and driven by filesystem events (no idle polling).
Late results must still match the live run and pending call. Watchers are released
on resolution, run replacement, pane removal, process exit and shutdown.

Notch navigation explicitly activates the macOS application (`cx.activate(true)`)
then raises the main window; selecting its tab/pane alone does not bring an
inactive application in front of another application.

Hover na automatycznym powiadomieniu zachowuje tryb powiadomienia, bez filtrów
i dodatkowych agentów. Dotyczy to także wygaśnięcia timera pod kursorem, nowej
zmiany statusu podczas hoveru oraz ponownego wejścia w trakcie zamykania. Dopiero
najechanie po pełnym zwinięciu otwiera pełny podgląd.


## Integration health

Session inspector separates agent activity from event-delivery health:
Connecting, Events received, No events received, Integration issue and Stopped.
A one-shot 12-second startup grace period marks a running pane with no root-agent
events as No events received. It is canceled on the first event, stop, pane
removal, replacement run or application shutdown. There is no idle heartbeat or
assumption that a quiet agent has disconnected. Events received describes observed
delivery and includes the count and last hook name, not a guarantee that every
provider hook is enabled.

Missing events cannot prove a Codex trust prompt is open. The diagnostic therefore
asks the user to check the terminal and /hooks rather than asserting approval is
required. Focus terminal selects the exact pane. Known relay/identity/question-watch
errors remain Integration issue; a process/tool error remains a separate activity
status. Notch's manual overview labels an unconnected startup No agent events;
health diagnostics do not invent agent-completion or attention notifications.

Resume Claude and multiple agents/worktrees were verified by the user. No automated
or GUI tests were run for this health change at the user's request; only formatting
and compilation were performed.

## Session reading layout

Session has one owned vertical ScrollHandle below the inspector tabs. Its content
stack and sections do not flex-shrink to the viewport, so long responses cannot
compress metadata or overlap recent activity/status bar. The scrollbar is mounted
outside the scrolling content. Changing pane/run resets the reading position;
ordinary agent updates preserve it. The selected session is prepared by observers,
not queried or reformatted repeatedly in render for unrelated agent events.
The Session shortcut transfers focus from the terminal to this list. Up/Down
select an active or unread session, Enter focuses its pane through AgentsState,
and Escape returns focus to the previous terminal. An empty list remains a
focused state so Escape still works.

Questions and last responses use the shared native MarkdownView with the compact
Canopy styles: headings, lists, inline/fenced code and tables. Markdown does not
introduce another vertical viewport. When the same session's response changes,
the previous rendered content stays until background preparation completes; a pane
change clears it first. Explicit HTTP(S) links can open in the browser. Relative
paths and file/app URLs are not activated; raw HTML/images follow the existing
inert/click-to-open Markdown policy.

Tool names use human labels (Bash / functions.exec_command → Terminal,
apply_patch → Edit files, AskUserQuestion → Ask a question). MCP tools retain a
readable server prefix; unknown tools get a readable name rather than a misleading
known-tool label. Recent hook events and permission modes are also formatted.
Profile names are looked up in the configured tool catalog. Raw IDs and untruncated
metadata remain available in tooltips. Presentation does not change hook matching,
permission policy, provider event data, session identity or resume behavior.

Validation: an isolated release fixture rendered the actual SessionInspector with
a long synthetic response. GUI checks covered metadata spacing, Markdown headings,
code/tables/checklists, top/bottom scrolling, 300/460-pixel sidebar widths, preserving
scroll during status update, empty pane state and resetting scroll on pane change.
No agent process or credentials were used. The fixture exited normally; its source
was removed from the checkout after verification. Label, safe-link and existing
agent-regression tests passed; GUI is not a measurement of physical frame rate.
