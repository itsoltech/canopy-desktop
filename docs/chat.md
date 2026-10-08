# Czat agentów (headless)

Moduł `canopy_desktop::chat` prowadzi rozmowę z agentem bez terminala: uruchamia
CLI jako subprocess z potokami, rozmawia strukturalnym protokołem i buduje
transkrypt neutralny wobec providera. Etap 1 obejmuje wyłącznie bibliotekę i testy;
widok pane'a `Chat` jest kolejnym etapem. Wariant terminalowy Claude/Codex jest
niezależny i nie zmienia zachowania.

## Warstwy

| Moduł | Odpowiedzialność |
| --- | --- |
| `chat::transcript` | `Transcript` z elementami `User`, `Text`, `Thinking`, `Tool`, `Permission`, `Notice`, `TurnEnd`; stabilne `ItemId`, `TurnId`; `Change::{Inserted, Updated}` dla wirtualizowanej listy |
| `chat::claude::wire` | typy stream-json Claude Code, tolerancyjne wobec nieznanych typów i pól; budowanie linii wyjściowych |
| `chat::claude::adapter` | czysty reducer zdarzeń Claude → transkrypt + sygnały dla sesji |
| `chat::session` | `ChatSession` bez I/O: status, kolejka wiadomości, zgody, przerwanie, wyjście procesu |
| `chat::launch` | `prepare_claude`: argv/env z tej samej ścieżki co terminal + flagi protokołu |
| `chat::process` | długożyjący subprocess: wątki stdin/stdout/stderr/wait, ograniczone zamykanie |

Właściciel (w etapie 2 encja widoku) przekazuje `process::Event` do
`ChatSession::on_event`, a zwrócone linie wysyła przez `Process::send`. Transkrypt
należy do właściciela; render czyta go bez I/O.

## Protokół Claude Code

Zweryfikowany na CLI 2.1.294 (`@anthropic-ai/claude-agent-sdk` 0.3.293 jako referencja).

- argv: profil (`ToolCatalog::launch` + `agent_config::prepare`), potem
  `--output-format stream-json --verbose --input-format stream-json
  --include-partial-messages --permission-prompt-tool stdio`. Bez `-p`.
- Argumenty narzędzia/profilu kolidujące z protokołem (`-p`, `--print`,
  `--output-format`, `--input-format`, `--permission-prompt-tool`,
  `--replay-user-messages`, `--sdk-url`, `-c`, `--continue`) dają jawny błąd.
- env: pełne środowisko z `ShellEnvironment` (host nie jest dziedziczony), bez
  `NODE_OPTIONS`, z `CLAUDE_CODE_SDK_READS_SESSION_STATE=1`. `CLAUDE_CODE_ENTRYPOINT`
  nie jest ustawiany — test live nie wykazał potrzeby.
- stdin: najpierw `initialize` bez `systemPrompt` (zostaje preset Claude Code),
  wiadomości użytkownika dopiero po potwierdzeniu.
- Zgody: `can_use_tool` → element `Permission`; odpowiedź `allow` z `updatedInput`
  (oryginalne wejście) i `toolUseID` albo `deny` z komunikatem. Kształt potwierdzony
  na żywo. `requires_action` poprzedza żądanie i sam nie zmienia statusu.
- Nieobsługiwane żądania (`hook_callback`, `mcp_message`) dostają `error`,
  `elicitation` → `decline`, `request_user_dialog` → `cancelled`.
- Wiele komunikatów `assistant` z tym samym `message.id` jest scalanych: tekst
  pochodzi ze `stream_event`, snapshot uzupełnia tylko wejście narzędzi i
  wiadomości niestreamowane (np. subagenci).
- Subagent w tle (`task_started.is_backgrounded`): wynik `tool_result` jest
  wewnętrznym placeholderem i nie jest pokazywany; wywołanie pozostaje `Running`
  do `task_notification`, którego `summary` staje się wynikiem. Wyjście subagenta
  ma `parent` = ID wywołania. Po zakończeniu zadania CLI samo zaczyna nową turę
  (bez `running`, gdy sesja nie była `idle`) — początek wiadomości głównego wątku
  poza otwartą turą otwiera turę agenta.
- Każda wiadomość użytkownika ma jawny `priority`: bez niego CLI wybiera `next` albo
  `later` według flagi rolloutu po stronie serwera. `later` = osobna tura po bieżącej,
  `next` = Steer: wiadomość wczytana po wyniku bieżącego narzędzia, w tej samej turze
  (gdy agent pisze już końcową odpowiedź, CLI uruchamia ją jako kolejną turę).
- `command_lifecycle {command_uuid, state}` (capability `msg_lifecycle_v1`) raportuje
  `queued` → `started` (moment wczytania) → stan końcowy. Dymek Steer powstaje przy
  `started`. `session_state_changed idle` jest jedynym sygnałem „agent skończył wszystko”
  (po wstrzymanych wynikach i subagentach w tle).
- Kolejka jest lokalna w `ChatSession`: wiadomości napisane podczas pracy czekają w Canopy
  i wysyłają się po jednej (`later`) po każdym `idle` (bez zdarzeń stanu: po `result`
  i stanach końcowych naszych uuid). Stop (`interrupt` z `cancel_queued`, gdy CLI ma
  `interrupt_cancel_queued_v1`) wstrzymuje kolejkę do kolejnej wysyłki lub Steer;
  Steer anulowany przed wczytaniem wraca do kolejki. Restart przenosi kolejkę wstrzymaną.
- Argumenty profilu wybierające sesję (`-r`, `--resume`, `--session-id`,
  `--fork-session`, `--resume-session-at`) też dają błąd: sesję wybiera tylko Canopy.
- Resume: `--resume <uuid>` z `PaneMetadata.resume_id`; ID sesji z `system/init`,
  `session_state_changed` i `result`.

## Hooki Canopy

Opcjonalny `HookConfig` dokłada obserwatora hooków tak jak wariant terminalowy
(`agents::launch::augment` + `environment`). Protokół pozostaje źródłem prawdy dla
transkryptu, zgód i statusu czatu; hooki zasilają `AgentsState`. W trybie headless
`SessionStart` przychodzi zaraz po starcie procesu (przed pierwszą wiadomością),
potem `UserPromptSubmit` i `Stop` z tym samym ID sesji.

## Proces i zamykanie

- `env_clear` + jawne środowisko, potoki, własna grupa procesów, odblokowana maska
  sygnałów w dziecku.
- Linie stdout do 16 MiB; dłuższe i nieczytelne dają diagnostykę, nie przerywają sesji.
- stderr: ogon 8 KiB; maskowane są wartości zmiennych o nazwach z KEY/TOKEN/SECRET/
  PASSWORD/CREDENTIAL (min. 8 znaków), z zapasem bufora, by sekret na granicy okna
  nie wyciekł fragmentem.
- `Exited` jest zawsze ostatnim zdarzeniem: wysyła je reader po EOF stdout. Gdy stdout
  trzyma potomek ignorujący SIGTERM, waiter raportuje wyjście po 2 s; późniejsze
  wiadomości sesja ignoruje.
- Po wyjściu lidera (także naturalnym) grupa procesów dostaje SIGTERM, zanim PID
  lidera zostanie zwolniony, więc narzędzia uruchomione przez agenta nie zostają.
- Waiter obserwuje wyjście bez zbierania procesu (`waitid` + `WNOWAIT`) i zbiera go
  pod blokadą, więc sygnał nigdy nie trafia w ponownie użyty PID.
- `close()`: koniec stdin → 2 s → SIGTERM grupy → 5 s → SIGKILL grupy. Drop bez
  `close` zamyka proces w tle. Przed zamknięciem właściciel woła
  `ChatSession::close_requested()`, aby kod wyjścia nie był pokazany jako awaria.
  Gdy status sesji jest zakończony (`Status::ended`, np. błąd `initialize`),
  właściciel zamyka proces. Wywołania w tle są wtedy anulowane.
- Windows: kompiluje się; zamykanie kończy tylko bezpośredni proces. Niezweryfikowane.

## Weryfikacja

```sh
cargo test --locked --lib chat::
cargo test --locked --test chat_process --test chat_claude_fixtures
# Live (płatne, model haiku, katalogi tymczasowe, ustawienia użytkownika wyłączone):
CANOPY_LIVE_CLAUDE=1 cargo test --locked --test chat_claude_live -- --ignored --nocapture --test-threads=1
# Nagranie nowych fixtures (redakcja konta, ścieżek domowych i tymczasowych):
CANOPY_RECORD_FIXTURES=/tmp/fixtures CANOPY_LIVE_CLAUDE=1 cargo test --locked --test chat_claude_live -- --ignored --test-threads=1
```

Fixtures w `tests/fixtures/chat/claude-<wersja>/` pochodzą z prawdziwego CLI. Przy
zmianie wersji CLI nagraj nowy katalog i porównaj wyniki testów.

## Pane czatu (etap 2)

- Wejście: „New chat” na górze lewego sidebara lub ⌘N otwiera paletę
  (`src/ui/new_chat_palette.rs`): „No project” (⌥⌘N) i otwarte projekty (⌘1…⌘9).
  `Chats::open_new(ChatTarget)` dodaje tab przez `ProjectsState::open_chat` i przełącza
  przestrzeń. Projekt → cwd projektu/worktree; „No project” → prywatny katalog
  `chat::scratch` (`<dane>/chats/<uuid>`, 0700), usuwany po potwierdzonym zamknięciu procesu
  zamkniętego taba (tylko bezpośrednie podkatalogi `chats/` o nazwie UUID, bez symlinków).
- Przestrzeń „No project” to `ProjectsState.scratch` (globalna encja `Workspace`, gdy nie
  wybrano projektu), zapisywana w `SessionSnapshot.scratch`; aktywny workspace to zawsze
  globalna encja (start terminali, widoczność dla notcha). Nowy tab `PaneKind::Chat`:
  `tool = "claude"`, konto = `profile_id`,
  wybory w `PaneMetadata.chat` (`ChatSettings { model, effort, mode }`, pole pomijane
  dla innych pane'ów). Tytuł „New chat” zmienia się na skrót pierwszej wiadomości.
- Rejestr `AppState.chats` (`src/app_state/chats.rs`) tworzy `ChatView` z `Window`
  dla czatów aktywnego taba po gotowości `AgentsState`; zamknięte pane'y przechodzą do
  retiring z jednym, współdzielonym zamykaniem procesu. `AppState::{running_workspaces,
  workspace_running, stop_workspace_processes}` łączą terminale i czaty (sidebar Stop,
  usuwanie worktree, merge target); quit czeka na `Chats::shutdown`.
- `ChatView` (`src/ui/chat/`) posiada `ChatSession`, `Process` i rejestrację hooków;
  pompa przetwarza zdarzenia partiami i robi jedno `notify` na partię. Wiersze
  (`rows.rs`) są zsynchronizowane z transkryptem przez `Change` i
  `MessageScrollerState::splice/remeasure_items`; każdy blok tekstu ma własny
  `MarkdownView::chat` (polityka `markdown::prepare`: bez obrazów i HTML, kopiowanie kodu,
  stream fade z tokenów MEDIUM/STAGGER, wyłączany przy Reduce Motion).
- Pickery w composerze: konto → model (modele z odpowiedzi CLI na `initialize`),
  reasoning (poziomy modelu), tryb (Ask / Accept edits / Plan / Auto / Yolo). Zmiany
  w trakcie: `set_model`, `apply_flag_settings{effortLevel}`, `set_permission_mode`;
  odmowa CLI cofa wybór z notką. Konto można zmienić tylko przed pierwszą wiadomością
  (nowy proces). Ostatnie wybory zapisuje `SettingsState::remember` (klucze `chat.*`).
- Proces zawsze startuje z `--allow-dangerously-skip-permissions`, żeby Yolo dało się
  wybrać w dowolnym momencie; samo to nie omija zgód.
- Restart w pane'ie zachowuje wiersze poprzedniej sesji nad separatorem „Session
  restarted”. Po restarcie aplikacji czat wznawia sesję (`--resume <uuid>` z hooków),
  ale historii jeszcze nie pokazuje (etap 6).
- Skróty (kontekst `Chat`): Enter wysyła, Shift+Enter nowa linia, ⌘. przerywa turę.

## Etap 2b: kolejka, subagenci, kod i pliki

- Belka kolejki (`src/ui/chat/queue.rs`) nad composerem: Steer, Edit (do pustego pola),
  Remove; Steer w locie pokazuje „Sending…” do `started`. ⌘⏎: zgoda → Allow, tekst w polu
  podczas pracy → Steer, puste pole → Steer najstarszej pozycji. Zamknięcie taba/pane'a/
  projektu i quit z niewysłaną kolejką przechodzi przez `Editors::guard` → `Chats::guard`
  (prompt „Discard queued messages?”).
- Subagenci: główna rozmowa pokazuje tylko elementy bez rodzica; wywołanie `Agent`/`Task`
  jest kartą (`src/ui/chat/subagent.rs`) ze stanem, licznikiem narzędzi i ostatnią
  aktywnością. Klik otwiera rozmowę subagenta w tym samym pane'ie (`Rows::thread`:
  prompt zadania, jego elementy, raport), z zablokowanym polem; Back/Esc wraca do miejsca
  przewinięcia (osobne `MessageScrollerState`). Zgoda subagenta jest w głównej rozmowie
  z etykietą „Subagent · …” i w jego wątku. Rozmowy subagentów poprzednich sesji (Restart)
  nie są dostępne.
- Bloki kodu: współdzielony highlighter (`src/ui/markdown/highlight.rs`) normalizuje tagi
  (`Rust`, `TS`, `zsh`, `jsx`/`tsx` → JS/TS) przed gramatykami GPUI Kit; nieobsługiwane
  języki zostają bez kolorów. Karta z ramką, w prawym górnym rogu język i Copy.
- Pigułki plików (`chat::mentions`): po zakończeniu bloku tekstu ścieżki z kodu inline
  i zwykłego tekstu (opcjonalnie `:linia`) wskazujące istniejący plik w katalogu czatu
  (po kanonizacji, bez wyjścia przez symlink) stają się linkami `canopy-file:` rysowanymi
  przez `MarkdownPlugin`. Wiersze Read/Edit/Write dostają pigułkę po sprawdzeniu pliku
  w tle. Klik: `FilesState::open_in` w aktywnym workspace + `Editors::reveal` (linia).

## Ograniczenia etapu 1

- (etap 1) Brak UI i persist/restore — dostarczone w etapie 2; zapis transkryptu
  do SQLite nadal w etapie 6.
- Thinking jest modelowany, ale CLI wysyła treść tylko przy włączonym wyświetlaniu
  podsumowań (`--thinking-display`); decyzja o fladze należy do etapu narzędzi/thinking.
- AskUserQuestion jest rozpoznawane (`PermissionKind::Question`), ale odpowiedzi
  z wyborem pojawią się w etapie zgód; obecnie możliwe jest tylko odrzucenie.
- Protokół jest wewnętrzny dla Claude Code i może się zmienić między wersjami.

## Etap 2c: druga runda poprawek

- Adapter: narzędzia, bloki i strumienie subagentów w tle przeżywają `result` tury głównej
  (ich wyniki przychodzą później); `task_notification` zamyka pozostałe dzieci danego
  subagenta. `SubagentHandback({message})` jest raportem subagenta (wiersz ukryty), także gdy
  jego wynik przyjdzie po powiadomieniu. `interrupt` po zakończonej turze zatrzymuje
  subagentów w tle (`task_notification stopped`, zweryfikowane na żywo).
- Subagenci uruchomieni razem są jednym wierszem z rozwijaną listą (`Kind::Subagents`).
- Zakładka nad composerem: wiersze „Waiting on subagent …” (po turze, gdy pracują
  subagenci w tle) i wiadomości w kolejce.
- Bloki kodu najwyższego poziomu (`markdown::split_code`) mają własne `TextViewState`
  w karcie z nagłówkiem (język, Copy), kolumną numerów i przewijaniem w poziomie; kod
  nie zawija linii. Zaznaczanie działa przez całą odpowiedź.
- Pigułki: każdy istniejący plik (względny lub bezwzględny), poza ukrytymi katalogami
  w domu użytkownika; spoza katalogu czatu otwiera się w swoim katalogu nadrzędnym.
- Narzędzia z pustym wynikiem lub „(empty)” nie są rozwijane; composer ma ikony providera,
  reasoningu i trybów.

