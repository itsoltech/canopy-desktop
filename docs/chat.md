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
- Pickery w composerze: model z reasoningiem (modele z odpowiedzi CLI na `initialize`, poziomy modelu)
  i tryb (Ask / Accept edits / Plan / Auto / Yolo).
- Picker modelu (`ui/chat/model_picker.rs`, wzorem ChatGPT): przycisk „Model Effort” (effort wyciszony;
  przy braku wyboru domyślny poziom konta, jeśli CLI go podaje) otwiera `Popover` sterowany przez
  `ModelPicker.open` (panel 272 px). Strona reasoningu: ikona providera, nazwa poziomu, nazwa modelu z `›`
  (przejście do listy), Reset (rotate-ccw, wraca do domyślnego; nieaktywny bez wyboru) i slider o stopach
  `effort_levels` (bez Default). Bez wyboru gałka stoi na domyślnym poziomie, gdy CLI go podaje (Codex);
  inaczej (Claude) gałki i wypełnienia nie ma, a nazwa brzmi „Default” — pojawiają się z przenikaniem przy
  pierwszym wyborze; → zaczyna od najniższego poziomu, ← od najwyższego. Wypełnienie to gradient
  `t::effort(0)` → `t::effort(intensywność)` (indygo → fiolet → róż), a przez nie płyną miękkie pasma światła
  (`paint_sheen`, `t::effort_sheen`): 56 px szerokości co 88 px, stałe 12 px/s (zmiana poziomu nie szarpie
  fazy), tylko w prostej części toru i wygaszane przed zaokrąglonymi końcami; jasność rośnie z poziomem
  (5 → 18%); przy ograniczonym ruchu stoją.
  Nazwa poziomu ma kolor końca gradientu i zmienia się animacją (stara odpływa, nowa wjeżdża o 8 px z dołu
  przy wzroście, z góry przy spadku, z przenikaniem, `STATE_CHANGE`). Klik, przeciąganie (globalne
  `MouseMove`/`MouseUp` z canvasu podczas trzymania) i ←/→ (focus panelu przez `track_focus`); `set_effort`
  dopiero po puszczeniu, gałka jedzie `STATE_CHANGE`.
  Strona modeli: „<konto> models”, Default z notą „Recommended · …”, modele z ✓, sekcja Accounts
  (te same reguły co wcześniej: restart przed pierwszą wiadomością, przełączenie z kontekstem, nieaktywne
  w trakcie pracy). Wybór modelu z poziomami wraca na stronę reasoningu, bez poziomów zamyka popover;
  model bez poziomów otwiera popover od razu na liście. Zmiana strony: wysokość panelu z poprzedniej do
  zmierzonej (RESIZE), treść wjeżdża o 14 px z prawej (do listy) lub z lewej (powrót) z przenikaniem. Zmiany
  w trakcie: `set_model`, `apply_flag_settings{effortLevel}`, `set_permission_mode`;
  odmowa CLI cofa wybór z notką. Konto można zmienić tylko przed pierwszą wiadomością
  (nowy proces). Ostatnie wybory zapisuje `SettingsState::remember` (klucze `chat.*`).
- Pickery zajmują miejsce, które zostawiają przyciski (`ChatView::pickers`, `overflow_hidden`). Canvas zapisuje
  szerokość tej grupy w `pickers_width`; render mierzy etykiety (`shape_line`, 12 px) i wybiera gęstość: pełne
  etykiety (model z effortem i tryb), sama nazwa modelu (tryb jako ikonka) albo same ikonki. Ukryta etykieta trafia do tooltipa
  i etykiety dostępności; załącznik i Send/Stop nigdy nie wychodzą poza composer.
- Composer „odpoczywa” podczas scrollowania rozmowy (jak `resting` w T3 Code, `ui/chat/resting.rs`):
  - liczy się zmiana pozycji listy, niezależnie od źródła (kółko, touchpad, przeciąganie scrollbara):
    `sync_resting` w każdym renderze porównuje `ChatScrollerState::position` / `scroll_top` z poprzednią klatką;
    ruch (zmiany w odstępach ≤ 200 ms) o łącznej drodze 24 px, gdy lista jest przewinięta w górę, zwija
    composer do jednej linii: początek szkicu (`set_auto_grow(1, 1)`), Attach i Send; pickery i kółko
    kontekstu znikają. Decyzję podejmuje czyste `Resting::observe` (testy w module);
  - nie liczą się: dopisywanie treści przy śledzeniu końca (lista nie jest przewinięta w górę), glide
    nawigatora i skoki na koniec (`ChatScrollerState::ends`);
  - tylko zwykły jednoliniowy szkic: decyzja, edycja, otwarte podpowiedzi, shell, błędy załączników lub polecenia
    oraz podwątek subagenta zostawiają pełny composer (i budzą zwinięty);
  - pełny composer wraca po przewinięciu w dół do końca (każdym sposobem), kółku w dół na samym końcu, Jump to latest / ostatnim kroku
    nawigatora, pisaniu i kliknięciu tekstu; pisanie w trakcie ruchu blokuje jego resztę (momentum);
  - wysokość wyspy przechodzi płynnie (RESIZE) od poprzedniej do zmierzonej (canvas w `Resting::measured`),
    bez animacji przy Reduce Motion.
- Przewijane elementy w rozmowie i nad nią (output narzędzi, thinking w trakcie, prompt podwątku, banner,
  decyzje, kolejka, kontekst handoffu) mają `.nested_scroll()`: kółko przewija je, a rozmowę dopiero na ich krawędzi.
  Lista rozmowy bierze kółko w fazie capture (`ScrollableMask` w `ChatScroller`, żeby bloki kodu z przewijaniem
  w poziomie nie zamieniały kółka na ruch w bok), więc zwykły listener wiersza nigdy by go nie dostał.
- Composer jest nakładką na dół rozmowy (jak `position: fixed; bottom: 0` z paddingiem treści,
  `ChatView::composer_overlay` w `render.rs`): banner, tacki i composer leżą w `absolute` kolumnie nad rozmową,
  która przewija się pod nimi. Composer i tacki mają nieprzezroczystą powierzchnię (`t::composer_surface()`,
  czyli `input_bg` zmieszane z `bg`), a pod nimi (bez 16 px paska scrollbara) rozmowa wygasa: 24 px
  gradientu nad nakładką (`COMPOSER_FADE`, bez `occlude`, więc kliknięcia przechodzą do rozmowy), potem
  prawie pełne `bg`. Obszar samej nakładki zatrzymuje kursor i kółko (`occlude`; jak element poza kontenerem
  przewijania strony — przewijają się tylko listy tacek). Viewport listy ma zawsze pełną wysokość, więc zmiana
  rozmiaru composera nie przesuwa przewiniętej rozmowy. Canvas mierzy wysokość nakładki do `overlay_height`,
  ale nie podczas odpoczynku composera i jego animacji: zwinięty composer zostawia pod rozmową miejsce
  pełnego, więc długość listy i scrollbar się nie zmieniają. Lista dostaje ją jako dolny padding (`ChatScroller::bottom_inset`), więc koniec rozmowy
  stoi tuż nad gradientem composera (lista nie ma już własnego dolnego fade'u). Jump to latest siedzi na górnej krawędzi nakładki takiej, jaka jest narysowana (`overlay_drawn`, mierzone także podczas odpoczynku), więc schodzi razem ze zwijającym się composerem. Scrollbar i maska kółka listy
  dostają `PaddedList` zamiast gołego `ListState`: jego liczby dla scrollbara pomijają padding, a na
  śledzonym końcu podają wysokość wierszy zamiast końca, więc pierwszy krok kółka z końca skakał o całe
  miejsce composera, a kciuk nie dochodził do końca. `PaddedList` liczy zakres z paddingiem (krótka lista:
  z granic pierwszego i ostatniego wiersza), a na końcu zwraca pełny zakres. Strony HTML i rail
  wiadomości są przycięte do widocznej części, a otwarte podpowiedzi, picker modelu i Reply chowają żywe
  strony (`pages_covered`), bo natywny widok rysuje się nad GPUI.
- Proces zawsze startuje z `--allow-dangerously-skip-permissions`, żeby Yolo dało się
  wybrać w dowolnym momencie; samo to nie omija zgód.
- Restart w pane'ie zachowuje wiersze poprzedniej sesji nad separatorem „Session
  restarted”. Po restarcie aplikacji czat pokazuje zapisaną historię i wznawia sesję
  (`--resume <uuid>`) — etap 6.
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
  przewinięcia (osobne `ChatScrollerState`). Zgoda subagenta jest w głównej rozmowie
  z etykietą „Subagent · …” i w jego wątku. Rozmowy subagentów poprzednich sesji (Restart,
  Edit, sprzed restartu aplikacji) otwierają się z zachowanego transkryptu.
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
  do SQLite — etap 6.
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

## Etap 3: narzędzia, diffy i thinking

- `chat::tools::view` zamienia zakończone wywołanie (wejście + `tool_use_result`) w `ToolView`:
  Shell (stdout/stderr, błąd), Read (zakres linii, treść), Change (hunki `structuredPatch`
  albo treść nowego pliku jako linie `+`, model `git::changes::DiffLine`), Web, Generic.
  Nieznany kształt → Generic. W CLI 2.1.294 nie ma osobnych narzędzi Grep/Glob/TodoWrite.
- Karty: `Rows` przygotowuje `ToolCard` przy zakończeniu narzędzia (podświetlanie podglądu
  Read poza renderem); diff Edit/Write rozwinięty, 12 linii + „Show more”.
- `turn_changes` składa zmiany Edit/Write tury (z subagentami) per plik; wiersz
  `Kind::Changes` po końcu tury z drzewem folderów i „Open diff” → `PaneKind::TurnDiff`
  (`TurnDiffTarget` w `resource`, `session` = numer epoki). Widok czyta transkrypt sesji
  (żywej, wcześniejszej albo zapisanej — etap 6) — to zapis tego, co zrobił agent, nie stan
  dysku. Zmiany przez Bash nie są liczone.
- Thinking: proces startuje z `--thinking-display summarized` (zastrzeżone w profilu);
  wiersz „Thinking…” na żywo, po końcu zwinięty do „Thought for Ns” (czas mierzony w widoku).
- „Worked for …”: po `TurnEnd` z `Outcome::Completed` `Rows::fold_turn` oznacza pierwszy wiersz
  pracy (`RowExtra.fold`) i resztę do końcowej odpowiedzi (`folded_by`); zwinięte wiersze mają
  zerową wysokość, rozwinięcie remeasure + fade. Wiadomości Steer w tej turze też są składane.


## Etap 4: zgody, pytania, plan i notch

- Wszystkie decyzje obsługuje panel nad composerem (`src/ui/chat/decision.rs`) w miejscu
  tacki kolejki: jedna prośba naraz, najstarsza pierwsza (`ChatSession::pending_decisions`).
  Tekst w composerze to własna odpowiedź na pytanie, uwaga przy odmowie albo uwagi do planu;
  `1–9` (przy pustym polu) wybiera opcję, `Enter` = Next/Submit (lub odmowa z uwagą),
  `⌘⏎` = pierwsza opcja, `⌘⌫`/Esc = odmowa / Skip / Keep planning. Draft composera wraca
  po ostatniej decyzji. W rozmowie zostaje wiersz śladu (`Permission.resolution`).
- `ChatSession::respond(item, Response)`: Allow, AllowForChat, Deny(uwaga), Answer
  (odpowiedzi per pytanie, kilka wyborów łączone „, ”), ApprovePlan{accept_edits}, KeepPlanning.
  Odpowiedź niepasująca do rodzaju prośby jest ignorowana.
- „Allow for this chat” (DEC-039): sugestie CLI (`permission_suggestions`) zamienione na
  `Remember::Rules` / `Remember::AcceptEdits` i odesłane zawsze z `destination: "session"`
  (`wire::session_permissions`) — CLI nie zapisuje `.claude/settings.local.json`. Reguły trafiają do
  `ChatSettings.allowed` i przy każdym starcie procesu do `permissions.allow` w prywatnej kopii
  ustawień profilu (`--settings`). Usunięcie (menu trybu) działa od następnego startu procesu.
- AskUserQuestion: `updatedInput.answers` (pytanie → tekst). ExitPlanMode (`PermissionKind::Plan`):
  akceptacja przełącza CLI w Default, wariant z edycjami dodaje `setMode acceptEdits` (session),
  „Keep planning” to odmowa z uwagami. Zaakceptowany plan rozwija się z wiersza śladu.
- Notch: czat zgłasza oczekującą decyzję przez `AgentsState::set_decision` (Needs attention,
  alert gdy czat niewidoczny) — prompty headless nie gwarantują hooka `PermissionRequest`.
  Wiersz notcha pokazuje tytuł czatu (`AgentsState::rename`).
- Własne hooki/ustawienia Claude Code użytkownika mogą auto-akceptować prośby; panel wtedy się
  nie pojawia. Weryfikacja: fixtures `permissions.jsonl`, `plan.jsonl`, `tests/chat_permissions.rs`,
  test live `rules_allowed_for_the_chat_stop_asking_and_survive_a_restart`.

## Etap 5: premium feel

- Akcje przy wiadomościach (`src/ui/chat/actions.rs`), widoczne na hover wiersza (`group_hover`):
  Copy (✓ przez 1,5 s), Edit przy wiadomości użytkownika (gdy nic nie pracuje), Retry przy ostatniej
  odpowiedzi (`ChatSession::retry_draft`), metryki tury z `TurnEnd` (czas, tokeny; kwota tylko gdy
  `init.apiKeySource != "none"`). Odpowiedź tury = ostatni `Text` głównego wątku (`RowExtra.turn_end`).
- Edit (`src/ui/chat/edit.rs`, DEC-043): punkt rozgałęzienia `fork_point` = `TurnEnd.resume_at` poprzedniej
  tury (uuid ostatniej wiadomości asystenta), dla pierwszej wiadomości świeża sesja
  (`ProjectsState::forget_chat_session`). Proces startuje z `--resume <sesja> --resume-session-at <uuid>
  --fork-session` (`launch::fork_at`); stara rozmowa nad separatorem „Edited — earlier version above”.
  Pliki: `CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING=1` w środowisku czatu; `rewind_files` (dry run → tacka
  z opcją „Also restore N files”, domyślnie wyłączoną; właściwy przed forkiem). Uuid wiadomości użytkownika
  (`UserMessage.uuid`) to klucz cofnięcia.
- Kontekst: `get_context_usage` po każdym `idle` (jedno zapytanie w locie, nie przy zamykaniu) →
  kółko w composerze z podpowiedzią i ostrzeżeniem od 80% przestrzeni przed buforem autokompaktowania.
- Notice: kompaktowanie z `compact_metadata` (trigger, pre_tokens), limit z godziną odnowienia
  i ponowienie API z odliczaniem (`Notice.until`, odświeżanie co 1 s tylko podczas odliczania).
- `components::AnimatedHeight`: mierzona wysokość z animacją RESIZE (panel decyzji, „Show more” w kartach).
  Drzewo zmian: `chat::tools::change_tree` (zagnieżdżone foldery, łączenie łańcuchów, sumy).
  Klawiatura: `KeyboardFocus::keyboard(label)` (tab_index + aria_label + focus_visible) dla klikalnych divów;
  przyciski GPUI obsługują Tab/Enter/Space same.
- Wydajność: długa rozmowa z nagrań (×60 → ~1260 wierszy, offline, w teście). Budowa wierszy
  od zera ~13 ms w release (`long_transcript_rows_build_quickly`, ignorowany); profil klatek w GUI
  wymaga przewijania w aplikacji (`./scripts/run-macos.sh profiling --features frame-profile`).

## Etap 6: sesje, historia, załączniki, nawigator

- Historia (DEC-045/046/049): `ChatSettings.id` (UUID czatu) → tabele SQLite
  `_canopy_rust_chat_epochs(chat, epoch, version, meta)` i `_canopy_rust_chat_items(chat, epoch, item,
  version, payload)` (wiersz na element, `src/settings/database/chat_history.rs`). Wiersze nowszej wersji
  są pomijane i nienadpisywane. `chat::history`: epoki (Restart/Edit → `Boundary`), kopia
  do zapisu ze skróceniem (wynik narzędzia > 64 KiB → początek+koniec, `structured` > 128 KiB → pominięte,
  `Item.trimmed`), `settle` (strumienie kończą się, narzędzia → Cancelled, decyzje → `Resolution::Expired`,
  otwarta tura → notka + `TurnEnd{Interrupted}`).
- Zapis: `AppState.chat_history` (`ChatHistory`) zbiera zmienione elementy z `ChatView::after_change`,
  jeden writer w tle (500 ms, koniec tury szybciej), flush w `finish_quit` przed zamknięciem czatów
  (błąd zostawia aplikację otwartą). Zamknięcie taba czatu (`Chats::retire`, także zamknięcie projektu)
  usuwa historię i `<dane>/chat-attachments/<id>`; po starcie `retain` usuwa czaty spoza layoutu.
- Odtworzenie: `ChatView::new` czyta historię, `start` czeka. Wcześniejsze epoki → `Rows.archived`
  (`Archived{epoch, transcript}` — diffy, subagenci działają), ostatnia → `ChatSession::restore_history`
  (`Transcript::restore`: identyfikatory i tury dalej, `fork_point`/Retry/`rewind_files` działają — sonda
  i test live `saved_history_continues_after_a_restart_and_restores_files`). Pane diffu tury tworzy uśpiony
  `ChatView` (bez procesu) dla swojego czatu.
- Załączniki (DEC-047/051/052): `UserMessage.attachments` (`Image{file, media_type, …}` / `Path`).
  `chat::attachments` skaluje obraz do 1568 px dłuższego boku (Lanczos3; PNG dla zrzutów, JPEG q85 dla zdjęć;
  ≤ 3,75 MB), zapisuje 0600 w folderze czatu; dane base64 tylko w pamięci, przy ponownym wysłaniu po
  restarcie czytane z pliku. W composerze załącznik jest odznaką w tekście (`InlineToken` GPUI Kit,
  tekst `[nazwa (rozmiar)]`, dla plików pełna ścieżka) — model dostaje ten tekst, obrazy jako bloki
  `image`; usunięcie odznaki usuwa załącznik. Strefa upuszczania obejmuje cały pane. Wejścia: ⌘V
  (`Textarea::on_paste`; obraz z przeglądarki — tekst-URL + natywny `NSPasteboard`), drop plików z Findera
  (GPUI nie przyjmuje przeciągania obrazów z przeglądarki), spinacz (`prompt_for_paths`). Limit 10 obrazów.
- Nawigator (DEC-048/050): własny `ChatScroller` (port `MessageScroller` z dostępem do `ListState`) +
  `navigator.rs`: kreska na wiadomość użytkownika od 2 wiadomości, wyróżniona bieżąca, hover wydłuża kreski
  i pokazuje kartę, klik/⌥⌘↑/⌥⌘↓ płynnie przewija (skok blisko celu + RESIZE; ręczny scroll przerywa,
  Reduce Motion → natychmiast).

## Etap 7: Codex

- Dostawca czatu = narzędzie pane'a (`Pane.tool`: `claude` / `codex`, `chat::provider::Provider`); picker konta
  pokazuje profile obu i zmienia narzędzie przed pierwszą wiadomością (`Workspace::set_chat_account`). ⌘N bierze
  ostatnie konto (`Chats::provider`).
- `codex app-server` (`launch::prepare_codex`: env profilu, klucz z Keychain, `-c profile=…`; bez hooków, bez zapisu
  do `~/.codex`). JSON-RPC bez pola `jsonrpc`, wiadomość na linię (`chat::codex::wire`). Start: `initialize` →
  `initialized` + `thread/start` | `thread/resume{threadId}` | `thread/fork{threadId,lastTurnId}` (Edit), `model/list`,
  `account/read`, `account/rateLimits/read`. Wiadomość: `turn/start` (tekst, `localImage` z folderu załączników,
  `mention` dla plików; tryb/model/effort jako nadpisania), Steer `turn/steer{expectedTurnId}`, Stop `turn/interrupt`.
- `ChatSession` ma rdzeń wspólny i gałęzie dostawcy (`session/codex_engine.rs`); adapter Codexa
  (`codex/adapter.rs`) wysyła te same `Signal`y. Mapowanie: commandExecution→`Bash`, fileChange→`Edit`/`Write`
  (wynik w kształcie structuredPatch z unified diff), webSearch→`WebSearch`, mcpToolCall→`mcp__server__tool`,
  turn/plan→`TodoWrite`, subAgentActivity→karta `Agent` (elementy wątku subagenta z `parent`), contextCompaction→
  Notice. Kolejka rusza po `turn/completed` (idle Codexa przychodzi przed nim).
- Tryby Codexa: Read only (`on-request`+`read-only`), Ask (`untrusted`+`workspace-write`), Auto (`on-request`+
  `workspace-write`), Full access (`never`+`danger-full-access`, ostrzeżenie, niezapamiętywany). Zgody: komenda,
  patch (podgląd diffu), uprawnienia; „Allow for this session” = `acceptForSession` (do restartu procesu); Deny z uwagą
  = `decline` + `turn/steer` z uwagą. Pytania `item/tool/requestUserInput` → panel (odpowiedzi po id pytań).
- Edit: fork wątku od poprzedniej tury, bez cofania plików (Codex nie ma checkpointów). Kółko: kontekst z
  `thread/tokenUsage/updated` i limit konta ChatGPT (`account/rateLimits`). Notch: `AgentsState::chat_activity`.
- Testy: `tests/chat_codex.rs` (nagranie `fixtures/chat/codex-0.161/session.jsonl`), live `tests/chat_codex_live.rs`
  (`CANOPY_LIVE_CODEX=1`): rozmowa, zgoda+odmowa, wznowienie, fork, komenda+patch, obraz.
- Granice: zmiany robione komendami nie trafiają do podsumowania zmian; Codex 0.161 często pyta tekstem zamiast
  narzędziem pytań; `app-server` jest eksperymentalny w Codexie.

## Etap 8: kilku agentów w jednej rozmowie

- Picker konta działa też w trakcie rozmowy (DEC-061): inne konto = przełączenie od następnej wiadomości
  (`ChatView::switch_agent`, `src/ui/chat/handoff.rs`). Wymaga spokojnego czatu (nic nie pracuje, brak decyzji
  i edycji); proces jest zamykany, nowa epoka dostaje separator `Boundary::SwitchedToClaude|SwitchedToCodex`,
  pane — narzędzie, konto i sesję do wznowienia (`Workspace::switch_chat_agent`). Niewysłana kolejka przechodzi
  wstrzymana (Resume). Przed pierwszą wiadomością zostaje zwykła zmiana konta (`set_account`).
- Epoki zapisują agenta (DEC-065): `EpochMeta.agent = AgentRef{tool, profile, session, settings}` — zapis przy
  starcie i ponownie, gdy sesja dostawcy dostanie id albo zmienią się wybory (`record_meta`). Starsze historie bez
  agenta przypisują wcześniejsze epoki pierwszemu znanemu agentowi czatu. Id sesji zapisujemy dopiero, gdy sesja
  ma wiadomość (pusta sesja CLI nie istnieje na dysku).
- Kontekst (DEC-062/066/067): `chat::handoff::summary` buduje deterministycznie skrót ≤ 24 KB — wiadomości
  (z nazwami załączników), końcowe odpowiedzi z nazwą agenta, zmienione pliki tury, notki przerwanych tur
  i ostatni plan; bez wyników narzędzi i thinking; najnowsze tury w całości, starsze skracane, najstarsze pomijane.
  Owinięty w `<canopy-context>`, idzie jako osobny pierwszy blok tekstu następnej wiadomości (`Draft.context` /
  `UserMessage.context`; Claude: blok `text` w `content`, Codex: element `text` w `input`). Sesja trzyma oczekujący
  kontekst (`ChatSession::set_handoff`) i dokleja go do pierwszej wiadomości rozpoczynającej turę (także z kolejki).
  Retry i Edit tej wiadomości wysyłają go ponownie.
- Powrót (DEC-063): agent z wcześniejszą sesją w tej rozmowie (to samo narzędzie i konto) ją wznawia i dostaje tylko
  części od swojej ostatniej; bez sesji — nowa sesja z pełnym skrótem i jego poprzednie wybory (model/effort/tryb),
  a za pierwszym razem domyślne konta z odpowiednikiem trybu. Gdy wznowienie się nie uda (proces kończy się przed
  pierwszą udaną turą; Claude: `No conversation found…`, kod 1), czat startuje od nowa z pełnym skrótem, ponawia
  wiadomość i mówi o tym notką.
- Podgląd (DEC-064): tacka „Context for X · N KB — sent with your next message” nad composerem (rozwijana,
  `AnimatedHeight`; ustępuje decyzji, edycji i kolejce). Po wysłaniu pod wiadomością jest zwinięty wiersz
  „Context from earlier in the conversation”. Kontekst nie jest zapisywany: po restarcie przed pierwszą wiadomością
  jest budowany ponownie z historii. Wiadomości z części innego agenta mają na hover „Sent to X” (Edit działa
  w części bieżącego agenta).
- Limit (DEC-064): notka limitu ostatniej, nieudanej tury ma „Continue with Codex/Claude Code”, gdy drugi dostawca
  jest dostępny: przełączenie na jego domyślne konto + ponowienie ostatniej wiadomości z kontekstem.
- Testy: `tests/chat_handoff.rs` (bloki kontekstu obu dostawców, kolejka/Retry, meta epok i stare historie, pane),
  live `tests/chat_handoff_live.rs` (`CANOPY_LIVE_CLAUDE=1 CANOPY_LIVE_CODEX=1`): Claude → Codex (zna słowo) →
  powrót do sesji Claude (wie, co utworzył Codex).
- Granice: reguły „Allow for this chat” działają tylko u Claude; skrót nie zawiera wyników narzędzi; kółko kontekstu,
  metryki i notch dotyczą bieżącego agenta.
- Grupowanie akcji (DEC-068): ciąg ≥ 2 akcji (thinking + udane narzędzia) zamknięty odpowiedzią tekstową agenta
  zwija się pod nagłówkiem „Ran 3 commands, read 2 files …” (`presentation::groups` / `actions_label`,
  `RowExtra.group/grouped_by` jak `fold/folded_by`, tworzone w `Rows::insert` przy wstawieniu tekstu). Edycje
  z diffem, plan (TodoWrite), subagenci, nieudane narzędzia, zgody i komunikaty przerywają ciąg i zostają widoczne;
  ostatni ciąg bez odpowiedzi zostaje rozwinięty. Działa w „Worked for”, wątkach subagentów i historii.
- Wyszukiwanie ⌘F (DEC-069, `ui/chat/find.rs`): pasek w prawym górnym rogu rozmowy, ⌘G / ⇧⌘G i Enter / ⇧Enter,
  Esc zamyka. Szuka (bez wielkości liter) w widokach tekstu wierszy (`MarkdownView::text_views`: wiadomości,
  odpowiedzi z kodem, thinking, plan, raport subagenta, części archiwalne) pokazanej rozmowy; karty narzędzi pomija.
  Trafienia to `TextViewState::set_range_highlights`, bieżące przewija `Rows::uncover` (otwiera fold/grupę/thinking)
  → `ChatScrollerState::reveal_row` → `reveal_range`. Liczenie tylko w handlerach, po zmianach treści z debounce.
- Cytowanie (DEC-070, `ui/chat/quote.rs`, `chat::quotes`): puszczenie myszy w rozmowie z zaznaczeniem
  (`TextSelection::selected_text`) pokazuje przycisk „Quote”; znika po odznaczeniu, przewinięciu, pisaniu i Esc.
  Cytat to `Attachment::Quote` z wzmianką `[❝ …]` w tekście (badge jak załącznik, tooltip z całością, ≤ 4 000 znaków),
  więc przeżywa kolejkę, Edit i historię. `quotes::expand` zamienia wzmiankę na blok `> …` w wire Claude/Codex
  i w skrócie handoff. Klik w badge wysłanej wiadomości przewija do źródła i na chwilę je podświetla.
  - Pasek nad zaznaczeniem ma dwie akcje: „Quote” (badge do composera) i „Reply”. Reply otwiera popover
    (`QuoteReply`, POPOVER, w miejscu zaznaczenia) z podglądem cytatu (3 linie) i własnym polem:
    Enter / Send wysyła od razu `Draft` z cytatem i odpowiedzią przez `send_draft` (szkic composera zostaje
    nietknięty; nieaktywne przy pustej odpowiedzi, czekającej decyzji, edycji i zakończonej sesji),
    ⌘⏎ / Ctrl+Enter i „Add to message” wstawiają badge i odpowiedź przy kursorze composera. Esc, klik obok
    i wysłanie zamykają popover, focus wraca do composera.

## Etap 9: komendy, wzmianki i polecenia w composerze

- Podpowiedzi (DEC-071, `ui/chat/suggest.rs`): `suggest::trigger` wykrywa słowo przy kursorze. Wyzwalacze:
  - `/` tylko na początku wiadomości;
  - `$`, `@` i `#` na początku słowa;
  - badge'e nie są częścią słowa.
  Tacka na górnej krawędzi composera, jak kolejka: wysuwa się (PANEL, `MotionReveal`), wysokość zmienia płynnie
  (`AnimatedHeight`) i chowa się z animacją. Tacka jest nakładką (`absolute`, `bottom: 100%` nad composerem,
  `occlude`, pod półprzezroczystym `input_bg` pełne `bg`): przykrywa rozmowę i inne tacki (edit, kolejka, kontekst), a lista, oś wiadomości i scroll się nie
  przesuwają. Prośba agenta (decision) chowa podpowiedzi. ↑/↓, Enter/Tab
  i Esc to akcje pola tekstowego (`MoveUp`, `MoveDown`, `Enter`, `IndentInline`, `Escape`), przechwytywane
  `capture_action` na composerze, bo akcje idą przed zwykłymi zdarzeniami klawiszy; tak samo działa ↑ historii.
  Lista jest liczona w subskrypcji `InputEvent::Change`, wyszukiwanie w tle z debounce i odrzucaniem starszych
  wyników. Wybór wywołuje `put_mention`, który zamienia zakres na `InlineToken`
  i dodaje gotowy `Pending`.
- `/`: lista z CLI.
  - Claude: `initialize.commands` (`claude::wire::commands`).
  - Codex: `skills/list` przy starcie i ponownie po `skills/changed` (`codex::wire::skills`).
  - Badge `Attachment::Command`. `chat::compose::expand` zamienia go na `/name` (Claude) albo na `$name` + wejście
    `{"type":"skill"}` (Codex).
- `$`: same skille (`CommandInfo.skill`) w dowolnym miejscu wiadomości.
  - Codex: wszystko z `skills/list`. Claude: przed pierwszą wiadomością wpisy `initialize.commands` bez
    `builtin`, potem dokładna lista `skills` z `system/init` (`config::mark_skills`; skill pluginu `plugin:name`
    pasuje do komendy `name`).
  - Badge `Attachment::Command { skill: true }` z wzmianką `[$name]`: fioletowy (`t::skill()`), ikona `box`,
    nazwa czytelna (`gpui-kit` → „Gpui Kit”), w tooltipie `$name` i opis. Rozwinięcie w miejscu badge'a jak
    przy `/`: `/name` dla Claude (model wywołuje skill narzędziem Skill), `$name` + wejście skill dla Codexa.
  - Lokalne komendy Claude (np. `/context`, `/compact`) odpowiadają syntetyczną wiadomością asystenta.
- `@`: `files::search::mentions` (ignore rules jak ⌘P, pliki i foldery, trafienia w nazwie wyżej). Badge to
  `Attachment::Path` z wzmianką `@ścieżka`: agent dostaje tylko ścieżkę (DEC-073).
- `#`: `IntegrationsState::pull_requests`:
  - repozytorium czatu z `task_repository` (nadpisanie Tasks, jeśli wskazuje GitHub, inaczej origin);
  - konto przez `account_for`, token z Keychain;
  - `Github::pulls`: otwarte PR-y albo `search` + `pullRequest(number:)`;
  - cache 60 s.
  Badge `Attachment::Pull`; agent dostaje `PR #N` i blok „Referenced pull requests”.
- `!` (DEC-072):
  - `!` wpisany w puste pole włącza tryb shella (`ChatView.shell`, `t::shell()`, etykieta „Shell”, placeholder
    „Run a shell command”) i nie zostaje w tekście; Backspace lub Esc w pustym polu wyłącza tryb, a po
    uruchomieniu polecenia tryb się kończy;
  - Enter uruchamia `chat::shell::Running`: `$SHELL -c` w katalogu czatu, środowisko z `ShellEnvironment`, własna
    grupa procesów, Stop wysyła SIGTERM, a po 2 s SIGKILL;
  - karta to `Body::Shell(ShellRun)` w osobnej turze, z outputem ograniczonym do 32 KB z każdej strony;
  - po zakończeniu `end_shell` wysyła (albo kolejkuje) wiadomość z `Attachment::Shell`: polecenie, kod wyjścia,
    output skrócony do ~5 KB z każdej strony. Wiadomość nie ma własnego dymka ani znacznika w nawigatorze, a tacka
    kolejki pokazuje ją jako „Output of !cmd” bez Edit;
  - Restart, przełączenie agenta i zamknięcie zatrzymują działające polecenia; po restarcie aplikacji karta
    pokazuje „Interrupted”.
- ↑ (DEC-075, `ui/chat/recall.rs`): w pustym composerze przywraca wcześniejsze wiadomości z badge'ami (również
  z archiwalnych części, bez wyników `!`). ↓ idzie do przodu, a edycja tekstu kończy przeglądanie.
- Granice:
  - polecenia `!` nie mają PTY ani wejścia;
  - Codex nie ma ręcznego `/compact` (kompaktuje sam);
  - `#` obejmuje tylko PR-y GitHuba.
- Zamykanie (QA etapu 9, DEC-076):
  - tab, pane, projekt i worktree z czatem, który ma rozmowę albo pracującego agenta, pytają „Delete this chat?” /
    „Delete N chats?”;
  - pytanie wymienia też pracującego agenta, niewysłaną kolejkę i działające polecenia `!`;
  - pusty czat zamyka się bez pytania;
  - quit niczego nie usuwa i pyta tylko o kolejkę;
  - usuwanie worktree mówi o czatach w kroku zamykania procesów.

## Etap 10: strony HTML w rozmowie

- Narzędzia agenta (DEC-076…078): Canopy jest serwerem MCP `canopy` (`canopy-desktop --chat-tools`, `chat::mcp`).
  Wpinamy go wyłącznie argumentami (`chat::launch::add_page_tools`):
  - Claude: `--mcp-config` i `--allowedTools mcp__canopy__show_html,mcp__canopy__preview_html`;
  - Codex: `-c mcp_servers.canopy.*`. Prośbę Codexa o zgodę na narzędzie (`mcpServer/elicitation/request`
    z `codex_approval_kind: mcp_tool_call`) adapter akceptuje tylko dla serwera `canopy`.
- `show_html {title, html, height?}` sprawdza limit 512 KB i odpowiada tekstem. Kartę rysuje czat z wywołania
  (`chat::page::SHOW_TOOL`), więc strona jest w historii SQLite. Karta nie wchodzi do grup akcji i zostaje widoczna
  pod zwiniętym „Worked for” (`RowExtra.page`). Skrót handoff ma tylko tytuł.
- `preview_html {html, width?}` idzie przez usługę stron (`chat::pages`):
  - gniazdo Unix 0700 w `/tmp/canopy-pages-*`;
  - token per proces czatu (env `CANOPY_PAGES_SOCKET`/`CANOPY_PAGES_TOKEN`), zwalniany przy zamknięciu;
  - termin 30 s;
  - render poza ekranem w aplikacji (`ui::chat::page::serve_previews`).
  Agent dostaje PNG 1x, `contentHeight` i konsolę.
- WKWebView (`native/html_page.m`, `ui::html_native`):
  - nietrwały magazyn danych;
  - origin `https://page.invalid/` (prawdziwe błędy JS, brak dostępu do `file:`);
  - linki i popupy do przeglądarki (http(s)/mailto);
  - kamera i mikrofon zablokowane, pobierania anulowane;
  - motyw jako zmienne CSS (`--background`, `--foreground`, `--muted-foreground`, `--card`, `--border`, `--accent`,
    `--red`/`--green`/`--yellow`, `--radius`, `--font-sans`/`--font-mono`), przezroczyste tło;
  - wysokość z `body` przez `ResizeObserver`, konsola przez `WKScriptMessageHandler`.
- Karta (`ui::chat::page`):
  - lokalne obrazy inline'owane w tle (`chat::page::inline_images`);
  - zrzut renderowany poza ekranem w skali okna, ponawiany przy zmianie szerokości;
  - karta widoczna na ekranie od razu dostaje żywą stronę, która się „rozgrzewa”: przezroczysta (`alphaValue` 0),
    przepuszcza kliknięcia i kółko (`hitTest` nil), ale ładuje się i odtwarza animacje. Pod kursorem, gdy nic jej
    nie zasłania (find, Quote, lightbox, dialog, prompt, drill), jest tylko odsłaniana (`set_active`), już
    ustabilizowana. Maks. 6 żywych; jako pierwsze odchodzą strony spoza ekranu;
  - zrzut powstaje po fontach i animacjach strony (`getAnimations().finished`, maks. 1,5 s), więc zgadza się
    z odsłoniętą stroną;
  - natywny widok ustawia warstwa malowana po wierszach (`page_layer`): przycięcie do rozmowy, chowanie
    niemalowanych w tej klatce;
  - kółko myszy nad stroną bez własnego przewijania przewija rozmowę;
  - ukryta strona oddaje fokus klawiatury;
  - akcje „Open in browser” (plik 0600 w `<dane>/pages`) i „Copy HTML”.
- Granice:
  - strona przyjmuje mysz tylko pod kursorem; poza nim widać zrzut;
  - pełne wsparcie tylko na macOS;
  - pliki w `<dane>/pages` nie są jeszcze sprzątane razem z czatem.

## Etap 11: cel `/goal`

Natywne `/goal` obu CLI (Claude Code ≥ 2.1.139, Codex ≥ 0.128). Agent pracuje turami, sam zaczynając kolejne,
aż warunek jest spełniony. Sprawdzone na żywo z Claude Code 2.1.295 i codex-cli 0.161.0 (DEC-079…082).

- Model (`chat::goal`): `Goal` (warunek, `GoalStatus`, liczniki), `GoalEnd` (karta), `GoalEvent` (co zgłosił
  agent). Stan celu żyje w `ChatSession::goal` (`session/goal_engine.rs`) i w `EpochMeta.goal` (restore).
- Tryb celu w composerze (`ui/chat/goal.rs`):
  - włącza go `/goal ` na początku wiadomości albo wybór `goal` z podpowiedzi `/` (wbudowane dla obu agentów);
  - `/goal` nie zostaje w tekście, a Backspace/Esc w pustym polu wychodzi z trybu (wspólne z trybem shell);
  - turkus `t::goal()`, etykieta „Goal” z licznikiem od 3600/4000 znaków, załączniki nieaktywne;
  - badge'e zamieniane na zwykły tekst (`plain_condition`).
- Claude (`/goal <warunek>` jako wiadomość):
  - cała praca nad celem to jedna tura i jeden `result`;
  - syntetyczne odpowiedzi CLI (`model: "<synthetic>"`) z `Goal set:`, `Goal active:`, `Goal cleared:`
    i `No goal set` zmieniają stan celu i nie są pokazywane jako odpowiedź;
  - syntetyczne wiadomości `Stop hook feedback:\n[<warunek>]: <powód>` to werdykty „not yet” (`Body::GoalCheck`);
  - po `result` z `terminal_reason: completed` Canopy cicho pyta `/goal`: `No goal set` kończy cel kartą;
  - Stop (`aborted_streaming`) zostawia cel ustawiony, a następna wiadomość wznawia pętlę;
  - werdykt met/impossible i liczby dopisuje odczyt `goal_status` z pliku sesji (`chat::claude::goal_record`,
    w tle, best-effort; bez pliku karta mówi „achieved”).
- Ciche komendy (`send_quiet`): `/goal`, `/goal clear` idą przed kolejką, bez dymka; ich `TurnEnd.quiet` jest ukryte.
- Codex (`thread/goal/*`, `origin: "user"`):
  - ustawienie to kroki `GoalStep`, każdy po odpowiedzi na poprzedni: `set {objective, paused}`, `turn/start`
    z warunkiem, `set {active}`;
  - kolejne tury Codex zaczyna sam: po `turn/completed` przy aktywnym celu sesja zostaje w `Working`
    (`goal_hold`, limit 5 s z widoku), kolejka nie wysyła, a nowa tura dostaje `Body::GoalTurn(n)`;
  - wiadomość z kolejki idzie do tury celu przez `turn/steer`;
  - `turn/interrupt` nie wstrzymuje celu, więc Stop najpierw wysyła `set {paused}`; Pause tylko wstrzymuje;
    Resume (`set {active}`) sam zaczyna turę;
  - przed `thread/resume`/`fork` jest `goal/get` i ewentualne `set {paused}`: wątek z aktywnym celem po
    wznowieniu sam zaczyna turę.
- Wiersz celu (`goal_bar`) nad tacką i composerem:
  - wysuwa się jak panel decyzji, pokazuje stan, zużycie (`goal::usage`), warunek (klik rozwija) i ostatni powód „Not yet”;
  - przyciski: Pause (Codex w pracy), Clear (Claude w pracy), Resume, Edit i ×;
  - po przełączeniu agenta zamiast niego wiersz „Previous goal … · Set goal” (`EpochMeta.previous_goal`).
- Koniec celu to karta `Body::GoalEnd`: achieved, impossible, cleared, przy przełączeniu agenta, przy edycji
  wiadomości albo po błędzie.
  - Przełączenie agenta kończy cel i wspomina go w skrócie.
  - Edycja wiadomości kończy cel przed rozwidleniem.
  - Kontynuowana rozmowa (powrót do agenta, edycja) czyści kopię celu w CLI: Claude cichym `/goal clear`,
    Codex `CodexStart.clear_goal` z pominięciem jego zgłoszeń do odpowiedzi.
  - Restart zachowuje cel bez uruchamiania go.
- Notch i inspektor sesji pokazują „Goal” zamiast „Working” (`AgentsState::chat_goal`). `Idle` z hooków
  w trakcie celu jest pomijane, więc powiadomienie przychodzi raz, na koniec.
- Granice:
  - teksty Claude nie są API (parsery i fixture `claude-2.1.295/goal.jsonl`);
  - bez ustawiania budżetu tokenów (DEC-081);
  - źle sformułowany cel Codexa potrafi kręcić tury bez końca: Pause/Stop zawsze dostępne;
  - zakładki i sidebar nie mają statusu czatu, więc „Goal” widać tylko w notchu i inspektorze.
