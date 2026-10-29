# Czat agentów (headless)

Docelowy refactor opisuje [plan abstrakcji agentów i remote](agent-runtime-remote-plan.md)
oraz [kontrakt odbioru](agent-runtime-remote-acceptance.md). To plan do implementacji;
runtime czatu należy już do `AppState.chats` (`ChatRuntime`), a nie do widoku; kod nadal
zawiera zależności od Claude/Codex.
Opis etapów poniżej jest historią dotychczasowej integracji, a nie planem nowego remote.

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
| `chat::session` | `ChatSession` bez I/O: status, Steer w locie, zgody, przerwanie, wyjście procesu, dowody doręczenia |
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
- Kolejka jest trwałym stanem hosta (CHAT-01/02, sekcja „Trwała kolejka hosta” niżej);
  `ChatSession` w trybie `use_host_queue` nie trzyma kolejki wiadomości, tylko Steer w locie.
  Dowodem doręczenia Claude jest `command_lifecycle started/completed` naszego UUID
  (`discarded/cancelled/refused` przed `started` = odmowa); bez lifecycle pierwsze
  zdarzenie tury po wysłaniu. `idle` bez dowodu daje niepewność, nie doręczenie.
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

- New chat na górze sidebara i ⌘N tworzą rozmowę w aktywnym projekcie/worktree,
  a bez projektu prywatną. Menu New private chat i ⌥⌘N zawsze wybierają prywatną rozmowę.
  Menu New chat in… i ⌘⇧N otwierają paletę (`src/ui/new_chat_palette.rs`) z wyborem
  kontekstu przed utworzeniem. Projekt → cwd projektu/worktree; bez projektu → prywatny
  katalog `<dane>/chats/<uuid>` (0700). Fork zachowuje tę samą paletę wyboru celu.
- Rozmowy projektowe są pod właściwym worktree w drzewie Projects, prywatne
  w Private chats. Wiersz pokazuje tytuł, agenta i status; kontekst/HEAD jest
  na rodzicu oraz w karcie na hover. Jeden wiersz to jeden Chat PaneId. Rename/Delete
  nadal działa na nieaktywnym workspace bez przełączania widoku. Centrum i panel
  narzędzi mają niezależny wybór, więc nawigacja rozmów nie zmienia terminala.
  Szczegóły: [układ workspace'u](features/workspace-layout.md).
- Pusty stan jest przy composerze: krótka podpowiedź zadania, a pod nią zwarte
  szczegóły rozmowy (`ui/chat/details.rs`, linie prowadzone ikoną jak w kartach
  sidebara): agent i model (`agent_name`, `model_label`), dla zdalnego czatu
  „On {komputer}”, projekt i HEAD z tagiem `default` albo „Private chat, no project”,
  folder roboczy (`~/…`, ucinany w środku) z Copy path. Lokalny czat bierze
  projekt z `Chats::workspace_of_chat` (inaczej po cwd) i HEAD z `GitState`;
  zdalny — workspace z podsumowania hosta i jego współdzielonego katalogu (ścieżka
  w postaci z tamtego komputera). Konto, wybór modelu i tryb zostają w pickerach
  composera. Wznowiony lokalny czat mówi wprost, że to stan stały: „Messages from before the resume
  aren't shown here. The agent still has the full conversation.” Szczegóły znikają z pierwszą wiadomością (rysuje je tylko pusty stan);
  widok odświeża się na zmiany `GitState` tylko, gdy rozmowa jest pusta. Agenci CLI
  mają własne wiersze w drzewie; ich wybór zachowuje terminal i nie tworzy rozmowy headless.
- New chat nie uruchamia się automatycznie przy starcie/przełączaniu kontekstu. Worktree
  bez tabów pokazuje przegląd z CTA ([komponenty](components.md#przegląd-worktree-bez-tabów)). New tab bez projektu tworzy czat; tworzenie terminala skrótem splitu wymaga projektu.
- Tytuł rozmowy należy do `PaneMetadata.title`. Opcjonalny `chat_title_state` odróżnia
  placeholder od nadanej nazwy; starsze zapisy używają nazwy taba jako fallbacku.
  Dock/swap/detach i zmiana nazwy grupy utrwalają ten fallback przed operacją. Ręczna nazwa
  nie jest nadpisywana pierwszą wiadomością. Model, widok, fork, diff i notch są synchronizowane.
- Usunięcie przechodzi przez Editors::guard_closing → Chats::guard. Nieznana/ładowana historia
  wymaga potwierdzenia bez uruchamiania agenta. Rewalidacja poprzedza skutki; odmowa nie
  zmienia kolejki. Guard czyta `ChatRuntime` i metadane pane'ów, więc obejmuje też czaty bez ChatView.
- Jedna operacja cleanup na PaneId blokuje dalszy zapis historii, czeka na agentów/starty/`!`,
  sprawdza użycie katalogu i odbiera wynik kasowania cwd, historii i załączników. Błędy zostają
  przy Chats z Retry cleanup; Stop i Quit dołączają do operacji, także bez widoku. Naturalny
  Exit shella nie zwalnia katalogu, jeśli grupa pozostaje niepotwierdzona; nie sygnalizujemy
  ponownie PID po reap. Nowe otwarcia plików/projektów w usuwanym katalogu są blokowane.
- Dokładny zakres i reguły: [samodzielne rozmowy](features/standalone-chats.md).
- Rejestr `AppState.chats` (`src/app_state/chats.rs`) trzyma jeden `ChatRuntime` na rozmowę,
  kluczem jest UUID czatu (`ChatSettings.id`), a nie PaneId (`src/app_state/chats/runtime.rs`,
  encja bez `Window`). PaneId jest tylko lokalnym kluczem prezentacji (`panes: PaneId → ChatId`)
  i nie trafia do protokołu. Runtime powstaje dla pokazanego czatu albo diffu tury po gotowości
  `AgentsState`, przez `Chats::ensure_runtime(pane)` albo `Chats::ensure_runtime_for_chat(uuid)`:
  ten drugi tworzy runtime z rekordu katalogu hosta bez pane'a, widoku i okna (zdalny Start/Send).
  Ten sam czat nigdy nie dostaje drugiej encji: pane otwarty później dla takiego czatu (`open_detached`
  z sidebaru) prezentuje istniejący runtime (`attach_pane`), a agent zostaje zarejestrowany pod
  kluczem z chwili startu. `ChatView` z `Window` powstaje tylko dla pokazanego pane'a i zostaje dla
  ukrytego taba; lazy start przy pokazaniu się nie zmienia.
- `Chats::sync_model` działa bez okna: zwykłe zamknięcie taba/pane'a odłącza widok,
  a jawne Delete uruchamia retiring z jednym zamykaniem procesu. Czat bez pane'a („detached”,
  np. utworzony zdalnie) zostaje, jest pokazany w sidebarze w swoim projekcie lub Private chats
  i znika tylko z projektem zamkniętym w trakcie działania Canopy (nigdy przy starcie). Guard
  zamknięcia projektu (`Editors::guard_closing_chats` → `Chats::guard_chats`) wymienia także te
  czaty, ich pracę, kolejkę i liczbę obserwujących urządzeń; `close_confirmed` odmawia, gdy lista
  zmieniła się po potwierdzeniu. Stop worktree, `running_workspaces` i Quit obejmują runtime'y bez
  pane'a. Delete takiego czatu ma to samo potwierdzenie i cleanup (`request_delete_detached`). `AppState::{running_workspaces,
  workspace_running, stop_workspace_processes}` łączą terminale i czaty (sidebar Stop,
  usuwanie worktree, merge target); quit czeka na `Chats::shutdown`.
- `ChatRuntime` posiada `ChatSession`, `Process`, pompę, generację, rejestrację hooków, `!` komendy,
  token stron, zapis historii, raporty notcha (decyzja, aktywność, goal) i close/shutdown. Pompa
  przetwarza zdarzenia partiami i robi jedno `notify` na partię. Każda zmiana sesji daje jedno
  `ChatRuntimeEvent::Changed{serial, changes}` (`take_changes` konsumuje tylko runtime); dalej
  `Archived` (Restart/Edit), `Restored` (historia), `Reset` (zmiana konta), `DraftReturned`, `Stopping`.
- `ChatView` (`src/ui/chat/`) posiada tylko prezentację: fokus, composer i szkic, wiersze, scroll,
  ruch, panel decyzji, edycję i wątki subagentów; intencje wywołuje na runtime. Widok zna kursor
  `(serial, liczba elementów)`: wstawia wszystko poza kursorem i odświeża zgłoszone `Updated`, więc
  nowy albo spóźniony widok dogania runtime bez kopiowania transkryptu. Wiersze
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
  Strona modeli (etap 16, wzorem T3): pasek kont, wyszukiwarka i lista z serduszkami — szczegóły niżej.
  Wybór modelu z poziomami wraca na stronę reasoningu, bez poziomów zamyka popover;
  model bez poziomów otwiera popover od razu na liście. Zmiana strony: wysokość panelu z poprzedniej do
  zmierzonej (RESIZE), treść wjeżdża o 14 px z prawej (do listy) lub z lewej (powrót) z przenikaniem. Zmiany
  w trakcie: `set_model`, `apply_flag_settings{effortLevel}`, `set_permission_mode`;
  odmowa CLI cofa wybór z notką. Konto można zmienić tylko przed pierwszą wiadomością
  (nowy proces). Ostatnie wybory zapisuje `SettingsState::remember` (klucze `chat.*`).
- Pickery zajmują miejsce, które zostawiają przyciski (`ChatView::pickers`, `overflow_hidden`). Canvas zapisuje
  szerokość tej grupy w `pickers_width`; render mierzy etykiety (`shape_line`, 12 px) i wybiera gęstość: pełne
  etykiety (model z effortem i tryb), sama nazwa modelu (tryb jako ikonka) albo same ikonki. Ukryta etykieta trafia do tooltipa
  i etykiety dostępności; załącznik i Send/Stop nigdy nie wychodzą poza composer.
- Komunikat pod polem (`ChatRuntime::notice`, `ChatNotice { kind: Error | Info, text }`): runtime ustawia go,
  gdy czegoś nie dało się zrobić (Error: zapis czatu, odmowa wiadomości, błąd hosta, plik spoza obrazów
  w zdalnym czacie) albo coś czeka (Info: agent bez Steer, wstrzymana kolejka, łączenie z komputerem).
  Wiersz `ChatView::notice_row` (`composer.rs`): `CircleX` czerwony albo `Info` wyciszony, tekst 12 px
  `secondary`, 20 px „Dismiss” (`icon_button`) woła `ChatRuntime::dismiss_notice`. Błąd zostaje do zamknięcia
  albo zastąpienia innym komunikatem; informacja znika przy następnej wysyłce (`ChatNotice::after_send`,
  lokalnie i zdalnie). Wejście i wyjście: wysokość (`MotionReveal`) i krycie razem, `STATE_CHANGE`
  jak wiersz kolejki; stan `NoticeRow` śledzi runtime w obserwatorze widoku i trzyma tekst do końca wyjścia.
  Treść błędów: „co się stało: powód”, np. „Couldn't save this chat: …”.
- Composer „odpoczywa” podczas scrollowania rozmowy (jak `resting` w T3 Code, `ui/chat/resting.rs`):
  - liczy się zmiana pozycji listy, niezależnie od źródła (kółko, touchpad, przeciąganie scrollbara):
    `sync_resting` w każdym renderze porównuje `ChatScrollerState::position` / `scroll_top` z poprzednią klatką;
    ruch (zmiany w odstępach ≤ 200 ms) o łącznej drodze 24 px, gdy lista jest przewinięta w górę, zwija
    composer do jednej linii: początek szkicu (`set_auto_grow(1, 1)`), Attach i Send; pickery i kółko
    kontekstu znikają. Decyzję podejmuje czyste `Resting::observe` (testy w module);
  - nie liczą się: dopisywanie treści przy śledzeniu końca (lista nie jest przewinięta w górę), glide
    nawigatora i skoki na koniec (`ChatScrollerState::ends`);
  - tylko zwykły jednoliniowy szkic: decyzja, edycja, otwarte podpowiedzi, shell, błędy załączników lub polecenia,
    komunikat błędu pod polem oraz podwątek subagenta zostawiają pełny composer (i budzą zwinięty);
  - pełny composer wraca po przewinięciu w dół do końca (każdym sposobem), kółku w dół na samym końcu, Jump to latest / ostatnim kroku
    nawigatora, pisaniu i kliknięciu tekstu; pisanie w trakcie ruchu blokuje jego resztę (momentum);
  - wysokość wyspy przechodzi płynnie (RESIZE) od poprzedniej do zmierzonej (canvas w `Resting::measured`),
    bez animacji przy Reduce Motion.
- Przewijane elementy w rozmowie i nad nią (output narzędzi, thinking w trakcie, prompt podwątku, banner,
  decyzje, kolejka, kontekst wysłany ze starszą wiadomością) mają `.nested_scroll()`: kółko przewija je, a rozmowę dopiero na ich krawędzi.
  Lista rozmowy bierze kółko w fazie capture (`ScrollableMask` w `ChatScroller`, żeby bloki kodu z przewijaniem
  w poziomie nie zamieniały kółka na ruch w bok), więc zwykły listener wiersza nigdy by go nie dostał.
- Composer jest nakładką na dół rozmowy (jak `position: fixed; bottom: 0` z paddingiem treści,
  `ChatView::composer_overlay` w `render.rs`): banner, tacki i composer leżą w `absolute` kolumnie nad rozmową,
  która przewija się pod nimi. Rozwinięta treść tacek (lista paska zadań, treść panelu decyzji, kolejka ponad pierwszy
  wiersz) zachodzi na rozmowę zamiast ją podnosić: `render::unfolds_over` odlicza ją od dolnego paddingu listy
  (`overlay_unfolded`, sumowane w tej samej klatce przed canvasem nakładki); nagłówki i composer zostawiają miejsce. Composer i tacki mają nieprzezroczystą powierzchnię (`t::composer_surface()`,
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
- Skróty (kontekst `Chat`): Enter wysyła, Shift+Enter nowa linia, ⌘. przerywa turę (⌘. i pozostałe komendy czatu można zmienić w Settings → Shortcuts).

## Etap 2b: kolejka, subagenci, kod i pliki

- Belka kolejki (`src/ui/chat/queue.rs`) nad composerem: Steer, Edit (do pustego pola),
  Remove; Steer w locie pokazuje „Sending…” do `started`. ⌘⏎: zgoda → Allow, tekst w polu
  podczas pracy → Steer, puste pole → Steer najstarszej pozycji. Usunięcie czatu z
  niedoręczoną kolejką przechodzi przez `Editors::guard_closing` → `Chats::guard`
  (pytanie wymienia liczbę wiadomości). Quit kolejki nie dotyka i o nią nie pyta.
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
- Tacka edycji mówi, co trwa, spinnerem (`edit::progress_row`, xsmall + 12 px `muted`; nieruchomy znak przy
  Reduce Motion): „Restoring files…” tylko przy zaznaczonym przywracaniu, „Sending the edit to {host}…”
  w zdalnym czacie, „Checking the message on {host}…” podczas ponownego podglądu (`Editing.checking`, nie
  `error`). Wysłanej edycji nie da się anulować (Cancel wyszarzony). Zdalny ołówek czeka na podgląd hosta jako
  `loading_icon_button` (`ChatView::edit_check`, `ButtonLoading`) i zostaje widoczny. Gdy host nie ma już
  podglądu edycji, tacka pokazuje „{host} no longer has this edit's preview.” z akcją „Check again”
  (`Editing.recheck`, ponowny `client_preview`); tę samą akcję ma nieudane sprawdzenie po restarcie.
- Kontekst: `get_context_usage` po każdym `idle` (jedno zapytanie w locie, nie przy zamykaniu) →
  kółko w composerze z ostrzeżeniem od 80% przestrzeni przed buforem autokompaktowania. Karta na hover ma
  miernik kontekstu i każdego limitu planu (procent, pasek, szczegół: tokeny / godzina odnowienia).
  Kółko czyta wspólny odczyt (`actions::context_reading`), nie `native()`: lokalny Claude/Codex
  podaje bieżące wartości, czat zdalny — `Usage.context_used/max/buffer` z ostatniego `TurnEnd`
  projekcji (opis: „after the last turn on {host}”), bez limitu planu. Brak danych = brak kółka,
  bez zastępczego „—”. Host wysyła dziś te pola puste (`remote/transcript.rs`), więc zdalnie kółko
  pojawi się dopiero, gdy host zacznie je wypełniać. ACP nie raportuje zużycia (`usage: None`).
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
- Zapis: `AppState.chat_history` (`ChatHistory`) zbiera zmienione elementy z `ChatRuntime::after_change`,
  jeden writer w tle (500 ms, koniec tury szybciej), flush w `finish_quit` przed zamknięciem czatów
  (błąd zostawia aplikację otwartą). Jawne Delete czatu (`Chats::retire`, także zamknięcie projektu)
  usuwa historię i `<dane>/chat-attachments/<id>`; po starcie `retain` usuwa czaty ani otwarte, ani zapisane
  na liście `/resume` z ostatnich 30 dni.
- Odtworzenie: `ChatRuntime::new` czyta historię, `start` czeka. Wcześniejsze epoki →
  `ChatRuntime::archived` (`Archived{epoch, transcript, agent, label}`, współdzielone przez `Rows.archived`
  — diffy, subagenci działają), ostatnia → `ChatSession::restore_history`
  (`Transcript::restore`: identyfikatory i tury dalej, `fork_point`/Retry/`rewind_files` działają — sonda
  i test live `saved_history_continues_after_a_restart_and_restores_files`). Pane diffu tury tworzy uśpiony
  `ChatRuntime` (bez procesu i bez widoku) dla swojego czatu.
- Załączniki (DEC-047/051/052): `UserMessage.attachments` (`Image{file, media_type, …}` / `Path`).
  `chat::attachments` skaluje obraz do 1568 px dłuższego boku (Lanczos3; PNG dla zrzutów, JPEG q85 dla zdjęć;
  ≤ 3,75 MB), zapisuje 0600 w folderze czatu; dane base64 tylko w pamięci, przy ponownym wysłaniu po
  restarcie czytane z pliku. W composerze załącznik jest odznaką w tekście (`InlineToken` GPUI Kit,
  tekst `[nazwa (rozmiar)]`, dla plików pełna ścieżka) — model dostaje ten tekst, obrazy jako bloki
  `image`; usunięcie odznaki usuwa załącznik. Strefa upuszczania obejmuje cały pane. Wejścia: ⌘V
  (`Textarea::on_paste`; obraz z przeglądarki — tekst-URL + natywny `NSPasteboard`), drop plików z Findera
  (GPUI nie przyjmuje przeciągania obrazów z przeglądarki), spinacz (`prompt_for_paths`). Limit 10 obrazów.
- Nawigator (DEC-048/050): własny `ChatScroller` (port `MessageScroller` z dostępem do `ListState`) +
  `navigator.rs`: kreska na wiadomość użytkownika od 2 wiadomości, wyróżnione wszystkie wiadomości użytkownika widoczne na ekranie
  (`ChatScrollerState::visible_rows`, bez części pod composerem; jak w ChatGPT), klik przewija wiadomość na górę widoku (ostatnie,
  pod którymi brakuje treści, tylko tyle, ile pozwala koniec rozmowy — bez sztucznego miejsca), hover wydłuża kreski
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

## Etap 8: kilku agentów w jednej rozmowie (wycofane, DEC-087)

- Agenta i konto wybiera się tylko przed pierwszą wiadomością (`ChatRuntime::set_account`, sekcja Accounts pickera
  znika, gdy `account_locked()`); `Workspace::set_chat_account` odrzuca pane z `resume_id`. Przełączenie w trakcie
  rozmowy, skrót kontekstu dla nowego agenta (`chat::handoff`), tacka kontekstu, „Continue with …” przy limicie
  i propozycja „Previous goal” zostały usunięte (etap 12).
- Rozmowy zapisane z przełączeniami nadal się otwierają (`tests/chat_saved_switches.rs`): separatory
  `Boundary::SwitchedToClaude|SwitchedToCodex`, agent każdej części (`EpochMeta.agent = AgentRef{tool, profile,
  session, settings}`, zapis w `record_meta`; starsze historie bez agenta przypisują wcześniejsze epoki pierwszemu
  znanemu agentowi), „Sent to X” na hover wiadomości z części innego agenta, zwinięty wiersz „Context from earlier
  in the conversation” pod wiadomością wysłaną z kontekstem (`UserMessage.context`; Retry i Edit wysyłają go
  ponownie) i zakończenia celu `ClearedOnSwitch`. Kontynuują z ostatnim agentem. Czat zapisany po przełączeniu,
  a przed pierwszą wiadomością, wraca do ostatniego agenta bez skrótu.
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
  W zdalnym czacie wyniki przychodzą stronami z indeksu hosta: licznik „N of M+” (tooltip „More matches load
  as you go”), gdy host ma kolejną stronę (`remote_next`); podczas pobierania spinner xsmall przy liczniku
  (także przy wybranym wyniku), a Next (przycisk i Enter) czeka na odpowiedź.
- Cytowanie (DEC-070, `ui/chat/quote.rs`, `chat::quotes`): puszczenie myszy w rozmowie z zaznaczeniem
  (`TextSelection::selected_text`) pokazuje przycisk „Quote”; znika po odznaczeniu, przewinięciu, pisaniu i Esc.
  Cytat to `Attachment::Quote` z wzmianką `[❝ …]` w tekście (badge jak załącznik, tooltip z całością, ≤ 4 000 znaków),
  więc przeżywa kolejkę, Edit i historię. `quotes::expand` zamienia wzmiankę na blok `> …` w wire Claude/Codex
. Klik w badge wysłanej wiadomości przewija do źródła i na chwilę je podświetla.
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
    nazwa czytelna (`gpui-kit` → „Gpui Kit”), w karcie na hover `$name` i opis (komenda: `/name` i opis; PR:
    tytuł, numer ze stanem, `head → base` i początek opisu). Rozwinięcie w miejscu badge'a jak
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
  - po zakończeniu `finish_shell` daje runtime'owi wiadomość z `Attachment::Shell` (trwała kolejka hosta): polecenie, kod wyjścia,
    output skrócony do ~5 KB z każdej strony. Wiadomość nie ma własnego dymka ani znacznika w nawigatorze, a tacka
    kolejki pokazuje ją jako „Output of !cmd” bez Edit;
  - Restart i zamknięcie zatrzymują działające polecenia; po restarcie aplikacji karta
    pokazuje „Interrupted”.
- ↑ (DEC-075, `ui/chat/recall.rs`): w pustym composerze przywraca wcześniejsze wiadomości z badge'ami (również
  z archiwalnych części, bez wyników `!`). ↓ idzie do przodu, a edycja tekstu kończy przeglądanie.
- Granice:
  - polecenia `!` nie mają PTY ani wejścia;
  - Codex nie ma ręcznego `/compact` (kompaktuje sam);
  - `#` obejmuje tylko PR-y GitHuba.
- Zamykanie (QA etapu 9, DEC-076):
  - zwykłe zamknięcie taba/pane'a pozostawia czat w katalogu i nie przerywa agenta;
  - jawne Delete, projekt i worktree z czatem, który ma rozmowę albo pracującego agenta, pytają „Delete this chat?” /
    „Delete N chats?”;
  - pytanie wymienia też pracującego agenta, niedoręczoną kolejkę i działające polecenia `!`;
  - pusty czat zamyka się bez pytania;
  - quit niczego nie usuwa i nie pyta o kolejkę: zostaje w bazie hosta i wraca wstrzymana.
  - odłączony czat pozostaje dostępny w sidebarze; Delete nadal usuwa go ze wszystkich urządzeń.

## Etap 10: strony HTML w rozmowie

- Narzędzia agenta (DEC-076…078): Canopy jest serwerem MCP `canopy` (`canopy-desktop --chat-tools`, `chat::mcp`).
  Wpinamy go wyłącznie argumentami (`chat::launch::add_page_tools`):
  - Claude: `--mcp-config` i `--allowedTools mcp__canopy__show_html,mcp__canopy__preview_html`;
  - Codex: `-c mcp_servers.canopy.*`. Prośbę Codexa o zgodę na narzędzie (`mcpServer/elicitation/request`
    z `codex_approval_kind: mcp_tool_call`) adapter akceptuje tylko dla serwera `canopy`.
- `show_html {title, html, height?}` sprawdza limit 512 KB i odpowiada tekstem. Kartę rysuje czat z wywołania
  (`chat::page::SHOW_TOOL`), więc strona jest w historii SQLite. Karta nie wchodzi do grup akcji i zostaje widoczna
  pod zwiniętym „Worked for” (`RowExtra.page`).
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
  - zdalny HTML otrzymuje ten sam CSS motywu wewnątrz izolowanej ramki, z zachowaniem
    oryginalnego tytułu i sugerowanej wysokości narzędzia. Ramka raportuje wysokość
    treści do wspólnego kontenera; nie tworzy osobnego obszaru przewijania. Zrzut
    czeka również na fonty i animacje dokumentu w ramce. Sandbox nadal ma tylko
    `allow-scripts`, a CSP blokuje połączenia sieciowe i zasoby spoza inlinowanych danych;
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
- Wiersz celu (`goal_section`, pierwsza część paska stanu z etapu 12) nad tacką i composerem:
  - wysuwa się jak panel decyzji, pokazuje stan, zużycie (`goal::usage`), warunek (klik rozwija) i ostatni powód „Not yet”;
  - przyciski: Pause (Codex w pracy), Clear (Claude w pracy), Resume, Edit i ×.
- Koniec celu to karta `Body::GoalEnd`: achieved, impossible, cleared, przy edycji wiadomości albo po błędzie
  (starsze rozmowy mają też `ClearedOnSwitch`).
  - Edycja wiadomości kończy cel przed rozwidleniem.
  - Kontynuowana rozmowa (edycja) czyści kopię celu w CLI: Claude cichym `/goal clear`,
    Codex `CodexStart.clear_goal` z pominięciem jego zgłoszeń do odpowiedzi.
  - Restart zachowuje cel bez uruchamiania go.
- Notch i inspektor sesji pokazują „Goal” zamiast „Working” (`AgentsState::chat_goal`). `Idle` z hooków
  w trakcie celu jest pomijane, więc powiadomienie przychodzi raz, na koniec.
- Granice:
  - teksty Claude nie są API (parsery i fixture `claude-2.1.295/goal.jsonl`);
  - bez ustawiania budżetu tokenów (DEC-081);
  - źle sformułowany cel Codexa potrafi kręcić tury bez końca: Pause/Stop zawsze dostępne;
  - zakładki i sidebar nie mają statusu czatu, więc „Goal” widać tylko w notchu i inspektorze.

## Etap 12: zadania agenta nad composerem

- Pasek stanu (`ChatView::status_bar`, `src/ui/chat/activity.rs`) to jedna powierzchnia na górnej krawędzi
  composera: wiersz celu (etap 11), a pod nim praca agenta. Model jest czysty i testowany (`chat::activity`):
  - `todos` — lista z `TodoWrite` (Claude, plan Codexa z `turn/plan/updated`) albo `TaskCreate` (id z wyniku
    `{task:{id}}`) i `TaskUpdate` (`status`, `subject`, `activeForm`; `deleted` usuwa); `version` to pozycja
    ostatniej zmiany listy;
  - `jobs` — wywołania głównego wątku z `tool.background`; subagenci w tle zostają w tacce „waiting for”, więc
    panel pokazuje polecenia i monitory.
- Linia zwinięta (DEC-084): chevron, postęp listy `2/5`, bieżące zadanie (`activeForm`, shimmer w pracy) albo
  pierwsze polecenie w tle, „N in background” albo „N finished in background” i × po zakończeniu wszystkiego.
  Rozwinięcie (`Disclosure` + `AnimatedHeight`) pokazuje sekcje Tasks i Background; czas działających poleceń
  liczy widok (`Instant` pierwszego zobaczenia, ticker co sekundę tylko przy otwartym panelu), zakończonych —
  `elapsed_seconds` albo ten pomiar. Klik w polecenie przewija do jego karty (`Rows::uncover` + `reveal_row`).
- Odczyt w `after_change` tylko po zmianie `Transcript::revision` lub sesji (`sync_activity`); render nie liczy list.
  Wysłanie wiadomości albo × chowa zakończone polecenia i ukończoną listę (DEC-085); nowa sesja (start, restart,
  restore) zaczyna z odłożonym tym, co już się skończyło.
- Stop (DEC-086): Claude zapisuje `task_id` z `system/task_started` (`Adapter::background_task`),
  `ChatSession::stop_background` wysyła `control_request {subtype: stop_task}`; odmowa daje notkę błędu,
  karta kończy się z `task_notification`. Codex nie ma żądania na pojedynczy terminal — bez przycisku.
- Codex: `commandExecution` wciąż `inProgress` po `turn/completed` (sesja unified exec) zostaje w mapie elementów
  jako `background`; `outputDelta` i `item/completed` po turze trafiają do tej samej karty (wcześniej spinner
  zostawał na zawsze, a wynik dublował się na końcu). Przerwanie, błąd tury i koniec procesu je anulują.
- Lista todo Claude: Claude Code 2.1.x wyłącza narzędzia list zadań dla nowych modeli
  (`CLAUDE_CODE_ENABLE_TODO_TOOLS`); Canopy ich nie włącza (DEC-083), panel pokazuje listę, gdy agent jej używa.
- Testy: `src/chat/activity.rs` (sekwencje list), `tests/chat_activity.rs` (nagrania
  `fixtures/chat/claude-2.1.296/{background,tasks}.jsonl`: polecenie w tle i `stop_task`, lista z TaskCreate/Update),
  `tests/chat_codex.rs::a_command_running_past_its_turn_ends_in_its_own_card`
  (`fixtures/chat/codex-0.161/background.jsonl`).

## Etap 13: bloki kodu w composerze

- ```` ``` ```` (opcjonalnie z językiem, `fences::opening`) i ⇧Enter w polu tekstowym (`ChatView::fence_enter`,
  capture `input::Enter` na wyspie) dzielą pole wokół nowego bloku kodu (DEC-088). Główne pole (`ChatView.composer`)
  zostaje ostatnie, pod wszystkimi blokami: podpowiedzi, badge'e, ↑, shell i cel działają jak wcześniej i tylko
  tam. `ChatView.code_parts` to bloki w kolejności, każdy z polem tekstu nad sobą (`ui/chat/code_blocks.rs`);
  puste pole nad blokiem (także między dwoma blokami) rysuje się tylko z fokusem — bloki stoją obok siebie,
  a strzałki otwierają między nimi linię, która znika, gdy zostanie pusta. Nie w trybie shell, celu i przy czekającej decyzji.
- Blok: lekkie pole `TextareaState` (`auto_grow(1, 12)`, Enter = nowa linia, `t::MONO` 13 px) w karcie z chipem
  języka (`picker_button` + menu `fences::LANGUAGES`). Bez podświetlania składni w composerze (DEC-090: edytor kodu
  gpui-kit był zbyt ciężki — powolne nowe linie, rynna numerów linii); wysłana wiadomość podświetla kod jak zwykle.
  Aliasy tagów (`rs`, `ts`, `py`, `sh`…) w `fences::grammar`, wspólne z podświetlaniem kodu w rozmowie.
- Klawisze (DEC-089/091, capture na karcie bloku): Enter i ⇧Enter — nowa linia, Tab — tabulator,
  ⌘Enter — `command_enter`; z bloku wychodzi się tylko strzałkami: ↑ na pierwszej / ↓ na ostatniej linii — pole
  nad / pod blokiem; wchodzi strzałkami (↑/↓ na krawędzi pola tekstowego) albo Backspace na początku pola pod
  blokiem (`field_backspace`); Backspace w pustym bloku — usuwa blok i łączy pola z badge'ami (`concat`).
- Szkic: `composer_draft` składa części `fences::join` (blok w fence'ach na własnych liniach), załączniki
  z tokenów wszystkich pól (`composer_tokens`). `set_composer_content` (Edit, ↑, Edit z kolejki, odzyskanie
  szkicu, `ComposerChange`) dzieli tekst `fences::split` (fence od początku linii, niezamknięty zostaje tekstem)
  i przenosi badge'e do pól (`slice`). `clear_composer` usuwa bloki; ↑ (recall), resting, `!` i `/goal` nie działają,
  gdy są bloki.
- Wiadomość użytkownika w rozmowie: `sent_markdown` escapuje tekst dosłownie, a fenced blocks przepuszcza jako kod
  (karta z językiem i Copy jak w odpowiedziach).
- Testy: `chat::fences` (split/join/aliasy), `ui::chat::code_blocks` (badge'e przy podziale i łączeniu),
  `ui::chat::attachments` (markdown wiadomości z blokiem). Klawisze i wygląd — QA w GUI.

## Etap 14: fork rozmowy do nowego czatu

- Pod zakończoną odpowiedzią agenta (akcje na hover) przycisk Fork (`git-branch`, „Fork into a new chat”),
  gdy tura ma punkt wznowienia (`TurnEnd.resume_at`), część ma zapisaną sesję agenta, a czat historię
  (`ChatView::fork_request`, `ui/chat/fork.rs`); także w częściach sprzed Restart/Edit, nie w wątkach subagentów.
- Klik: `ChatForkRequested` → `Chats` → widok workspace otwiera „New chat in” w trybie „Fork into”
  (`NewChatPalette` z `Purpose::Fork`). Wybór → `Chats::fork(target, request)`.
- Kopia (DEC-092): `ChatHistory::fork` czyta historię źródła z bazy razem z wierszami czekającymi na zapis,
  tnie ją `chat::fork::cut` (wcześniejsze części całe, cięta do `TurnEnd` odpowiedzi włącznie, z wyjściem
  subagentów; ostatnia: `continued`, bez celu, `fork_at = resume_at`), kopiuje obrazy, które nazywają skopiowane
  wiadomości (`fork::images`), i zapisuje od razu pod nowym id czatu. Dopiero wtedy powstaje tab „<tytuł> (fork)”
  z tym samym narzędziem, kontem i wyborami, `resume_id` = sesja źródłowej części, bez zapamiętanych zgód.
- Agent (DEC-093): `start()` bierze `fork_at` z mety przywróconej części (albo z `ChatRuntime.fork_at`) i rozgałęzia
  sesję jak Edit: Claude `--resume <źródło> --resume-session-at <uuid> --fork-session`, Codex `thread/fork`
  z `cwd` nowego projektu. Punkt trzyma się, dopóki nowa sesja nie dostanie id (`save_session` czyści
  `fork_at`, `record_meta` zapisuje): Restart przed pierwszą wiadomością i restart aplikacji rozgałęziają znowu,
  zamiast kontynuować sesję źródła. Nowa część po Restart zapisuje ten sam punkt (`EpochMeta.fork_at`).
- Sondy (2026-10-14): Claude 2.1.296 rozgałęzia sesję z innego folderu projektu (także w punkcie) i zachowuje uuid
  wiadomości; Codex 0.161 `thread/fork` z innym `cwd` wie tylko to, co było do punktu, i zachowuje id tur —
  Edit/Retry w forku działają u obu.
- Testy: `chat::fork` (cięcie, obrazy, zapis `fork_at` i stare mety). GUI — QA.

## Etap 15: `/resume` — zamknięte czaty wracają

- Lista zamkniętych czatów: `chat::closed::ClosedChat` (tytuł taba, narzędzie, `PaneMetadata` z sesją
  i wyborami, projekt: nazwa i ścieżka) w tabeli `_canopy_rust_closed_chats(chat, version, closed_at, payload)`.
  Zapis przy zamknięciu (`ChatHistory::close`, pane z `ChatRuntime`) jest wstrzymany razem z DEC-095 — zamknięcie
  usuwa rozmowę (`chats/cleanup.rs`); lista pokazuje wcześniej zapisane czaty.
- `/resume` (DEC-094): komenda Canopy w podpowiedziach `/` (zastępuje wpis Claude, który headless nie działa);
  wybór usuwa słowo z pola i zgłasza `ChatResumeRequested { replace }` (pusty czat, w którym ją wpisano) →
  `Chats` → workspace otwiera `ResumePalette` (`ui/resume_palette.rs`): wyszukiwarka (tytuł, projekt), czaty
  tego projektu (po ścieżce worktree/projektu), Tab — wszystkie projekty, wiersz „agent · projekt · 2h ago”,
  kosz (`Chats::forget`: historia, załączniki, folder „No project”).
- Wybór: `Chats::reopen` otwiera czat z zapisanym `PaneMetadata` (to samo `chat.id`, `resume_id`, cwd) w jego
  projekcie, jeśli jest otwarty, inaczej w bieżącej przestrzeni; zdejmuje go z listy (`ChatHistory::reopened`);
  pusty czat, w którym wpisano `/resume`, jest zamykany. Historia i sesja agenta wracają jak po restarcie aplikacji.
- Czyszczenie przy starcie (`ChatHistory::retain`): `retain_chat_history(keep, keep_since)` usuwa czaty ani
  otwarte, ani zamknięte w ostatnich 30 dniach (`closed::KEEP_DAYS`); załączniki zamkniętych zostają, foldery
  „No project” wygasłych są usuwane.
- Testy: `settings::database::chat_history` (lista, wygasanie, reopen, delete), `chat::closed` (zapis, projekt,
  wyszukiwanie). GUI — QA.

## Etap 16: wybór modelu jak w T3

- Strona modeli pickera (DEC-097): po lewej pasek (48 px) — ♥ ulubione, potem po jednym przycisku na konto
  (znak dostawcy, przy kilku profilach tego samego agenta inicjały profilu; tooltip z nazwą); po prawej
  wyszukiwarka (`ModelPicker.search`, fokus i czyszczenie przy otwarciu) i lista wybranego konta: Default
  z notą „Recommended · …”, potem modele. Wiersz: nazwa (✓ przy wybranym), konto na zakładce ulubionych, ⌘1…⌘9,
  serduszko (klik dodaje/usuwa, DEC-099). ↑/↓ i Enter wybierają z klawiatury (`models_key`, `capture_key_down`);
  w kontekście `ModelPicker` ⌘1/⌘2 nie przełączają kart (`NoAction`). Panel ma 272 px na stronie reasoningu
  i 388 px (340 bez paska) na liście; szerokość przechodzi razem z wysokością.
- Wybór modelu innego konta przed pierwszą wiadomością: `set_account(provider, profile, model)` — restart
  pustego czatu na nowym koncie od razu z tym modelem. Po pierwszej wiadomości (DEC-100) nie ma paska ani
  ulubionych: tylko modele bieżącego konta.
- Listy modeli (DEC-098, `app_state::ModelCatalog`, `chat::models`): pamięć w tabeli
  `_canopy_rust_models(id=1, version, payload)` (JSON `Catalog`: modele per profil i ulubione), czytana przy
  starcie. Picker pokazuje ją od razu; każde otwarcie odświeża w tle konta, których agent nie podał już list
  w tym czacie (`ModelCatalog::refresh`: proces CLI konta przygotowany jak czat — `prepare_claude` bez hooków
  / `prepare_codex` — w folderze czatu; Claude `initialize`, Codex `initialize` + `model/list`; limit 15 s,
  zamykany po odpowiedzi, jedno pobieranie na konto naraz). Inna lista podmienia się bez migania i jest
  zapisywana; listy z działających sesji też trafiają do pamięci (`ChatView::report_models`). Przed startem
  agenta `ChatView::models` bierze listę z pamięci. Konto bez pamięci: „Loading models…”, błąd: „Couldn't
  load models: …” (zostaje Default).
- Testy: `chat::models` (pamięć list, serduszka, odpowiedzi, wyszukiwanie),
  `settings::database::chat_history` (jeden wiersz katalogu), `tests/chat_models_live.rs` (live, ignorowane:
  `CANOPY_LIVE_CLAUDE=1 CANOPY_LIVE_CODEX=1 cargo test --locked --test chat_models_live -- --ignored`).
  GUI — QA.

### Neutralne backendy i ACP v1

`chat::agent::AgentDefinition` oddziela ID/nazwę/profil agenta od `BackendKind`
(Claude, Codex, ACP). Dotychczasowy `Provider` pozostaje zgodnym API natywnego
UI. `backend_session::BackendSession` udostępnia wspólne operacje runtime'u,
capabilities i opcje; natywne rozszerzenia nadal obsługuje `ChatSession`.
`BackendSession::submit` (`ChatSession::submit_accepted`, `AcpSession::submit`)
przyjmuje stabilne ID wiadomości hosta i odrzuca zajętą sesję (`ready_for_message`);
nie dodaje drugiej kolejki. `take_receipts` zwraca dowody: `Delivered`, `Refused`
albo `Unknown` (koniec runu/`idle` bez dowodu). Trwałość i deduplikacja należą do hosta.

`ToolCall.kind` jest semantycznym wynikiem adaptera. `None` oznacza stary zapis
bez tego pola i używa zgodnego klasyfikatora nazw, bez migracji historii.
`Some(Other)` oznacza jawnie nieznane narzędzie, również gdy jego tytuł przypomina
znaną nazwę. Codex zachowuje własne nazwy protokołu zamiast nazw narzędzi Claude.
Karty i grupowanie korzystają z `semantic_kind()`, a ścieżki z `file_path()`.
Wspólne wyniki inicjalizacji, użycia kontekstu, lifecycle i podglądu rewind
należą do `chat::backend`; stare ścieżki importu Claude mają zgodne reexporty.

`acp::AcpSession` implementuje JSON-RPC ACP **v1** po stdio, zgodnie z
[protokołem inicjalizacji](https://agentclientprotocol.com/protocol/v1/initialization)
i [konfiguracji sesji](https://agentclientprotocol.com/protocol/v1/session-config-options).
Runtime tworzy jeden `Process` przez `launch::prepare_acp`, przekazuje zdarzenia
oraz zapisuje zwracane linie. Ten adapter nie wykonuje I/O. Host rozwiązuje
środowisko konta/profilu przed launch; launcher waliduje argv, zachowuje izolowane
środowisko i redakcję wartości sekretów. Zewnętrzne session/model/option/request
IDs są nieprzezroczyste, a string i numeric JSON-RPC IDs pozostają rozróżnione.

Obsługiwane są initialize, new/load, prompt, strumień tekstu/thinking/narzędzi,
plan, komendy, cancel, permission options, config options oraz ogłoszone modele
i tryby. Obrazy wejściowe wymagają ogłoszonego image capability i danych dostępnych
na hoście. Brak wsparcia resume kończy start błędem zamiast tworzyć nową sesję.
Steer, fork, rewind i goal dla ACP nie są emulowane promptami. Zmiana konfiguracji
pozostawia poprzednią wartość do odpowiedzi; odmowa nie usuwa szkicu.

Podczas load istniejący checkpoint pomija replay transkryptu. Bez checkpointu
import jest buforowany (limit 32 MiB) i publikowany dopiero po udanej odpowiedzi
load; błąd lub exit odrzuca całość. Opcje mogą być odświeżane niezależnie.
Cancel odrzuca oczekujące decyzje i czeka na zakończenie oryginalnego promptu.
Pierwsza odpowiedź na zgodę wygrywa, a ID wybranej opcji wraca bez interpretowania
etykiety. `allow_always` nie jest przedstawiane jako zgoda ograniczona do czatu.
Wyjście procesu nie uruchamia automatycznego retry wiadomości.

HostCapabilities domyślnie nie ogłasza plików ani terminala. Po zainstalowaniu
rzeczywistych handlerów runtime włącza je jawnie. Żądania `fs/read_text_file`,
`fs/write_text_file`, `terminal/create`, `output`, `wait_for_exit`, `kill` i
`release` trafiają przez typowane `HostRequest` do usług hosta. Adapter sprawdza
sesję, limit 128 pending requests, parametry i pojedynczą odpowiedź. Host nadal
odpowiada za zakres ścieżek, bufory edytora, dirty/conflict, limity outputu,
terminal ownership i cleanup runu. Nie wolno ogłaszać tych capabilities na
podstawie samych testów adaptera.

Offline `tests/chat_acp.rs` używa fake procesu i kontrolowanych zdarzeń bez
modeli, GUI ani sieci. To dowód protokołu/state machine, nie odbiór całego E2,
rzeczywistego agenta ACP, usług hosta ani macOS/Windows/mobile.

### Runtime, UI i usługi hosta dla ACP

`ChatRuntime` (`app_state/chats/runtime.rs`) trzyma `BackendSession`. Wspólne
ścieżki (start, pompa zdarzeń, transkrypt, historia, status, decyzje, Stop,
cleanup, trwała kolejka) działają dla każdego backendu; rozszerzenia Claude/Codex
(goal, Steer, rewind/edycja, `!`, modele i tryby, page tools) idą przez
`native()` i nie istnieją dla ACP. UI ukrywa je według `capabilities()`:
`/goal` tylko z `goal`, Steer z `steer`, fork i edycja tylko dla natywnych
sesji, `!` tylko dla Claude/Codex.

Agent ACP to narzędzie typu Custom z polem `acp: true` w katalogu narzędzi
(opcjonalne pole JSON, bez migracji bazy; Preferences → Tools → checkbox „Chat agent”
z etykietą klikalną razem z polem i pomocą „Opens in a chat instead of a terminal. The
program must support Agent Client Protocol (ACP).”; „Enable tool” to ten sam
`labeled_checkbox`). `ToolCatalog::agent_definition` buduje `AgentDefinition` z
executable, argumentów narzędzia, profilu i pane. Narzędzie bez profili jest
jednym kontem o ID narzędzia. Konta ACP pojawiają się obok Claude/Codex na
szynie wyboru konta (przed pierwszą wiadomością); taki agent nie trafia do menu
terminali. Composer czatu ACP zamiast pickerów modelu i trybu ma jedno menu
opcji agenta: `configOptions`, a gdy ich brak — `models`/`modes` z ACP, z
nieprzezroczystymi ID. Bez opcji menu nazywa agenta: „Options appear once {agent}
starts” przed startem, „{agent} has no options” po nim. Zmiana czeka na odpowiedź agenta; odmowa zostawia
poprzednią wartość i komunikat pod composerem. Potwierdzone przez agenta wartości
opcji trafiają do `ChatSettings.agent_options` (nieprzezroczyste ID opcji i wartości,
także `canopy:model`/`canopy:mode`). Nowa sesja dostaje je po kolei, po jednej zmianie
naraz, tylko gdy agent nadal oferuje daną wartość; odmowa zostawia wartość agenta.

Panel decyzji pokazuje opcje zgody ACP z ich nazwami; opis mówi tylko „jednorazowo”
albo „agent zapamiętuje wybór” — bez przypisywania zakresu czatu. Esc/⌘⌫
wybiera `reject_once`, potem `reject_always`, a bez nich anuluje prośbę. Notatka
do odmowy nie istnieje w ACP, więc tekst w polu idzie po odmowie jako zwykła
wiadomość: Enter albo przycisk „Reject and send” (`primary`, tooltip „Enter”)
wybiera odmowę agenta w tej samej kolejności co Esc i wysyła tekst przez `submit`
(w trakcie tury czeka w kolejce). Pod polem podpowiedź „{Agent} can't take a note
with its answer. Your text goes as the next message.”, a opcja odmowy ma dopisek
„Enter sends your text after it”. Puste pole: „Choose an option, or write a reply
to reject”.

ACP korzysta z tej samej trwałej kolejki hosta co Claude/Codex. Dowodem doręczenia
jest pierwsze `session/update` lub żądanie agenta w turze promptu albo odpowiedź
na `session/prompt`; błąd odpowiedzi przed zdarzeniem tury to odmowa. Resume ACP
zapisuje się tylko, gdy agent ogłosił `loadSession`.

### Trwała kolejka hosta

`runtime/queue.rs` jest jedynym dispatcherem wiadomości czatu (lokalnych i zdalnych):

- Start aplikacji (`SettingsState::new`) po otwarciu bazy, przed narzędziami, woła
  `initialize_runtime` (lease hosta, jedna migracja, `attempting` → `delivery_unknown`,
  wszystkie kolejki wstrzymane) i `load_runtime_chats`. Host ID, rekordy czatów i
  `ArtifactStore` (otwierany w tle, `<data>/artifacts`, sprzątanie co 60 s w tle,
  zatrzymywane przy quit) trzyma `SettingsState`. Błąd startu runtime'u zatrzymuje start jak błąd ustawień.
- Runtime zapisuje `ChatRecord` (agent, konto, workspace/private, `history_epoch`
  bieżącej części) przed przyjęciem Send; wysyłka czeka na zapis rekordu tej części.
  Rekord z `recovery_reason` (adoptowana historia bez metadanych) blokuje start i Send
  z komunikatem — nigdy nie wybieramy domyślnego konta.
- Send: `QueueDraft::command_for_message` (ID wiadomości + szkic + `SendContext` rekordu)
  → `accept_runtime_message` atomowo zapisuje receipt i pozycję. Lokalne receipty są
  potem potwierdzane (`acknowledge_runtime_command`). Wstrzymana kolejka bez innych
  oczekujących pozycji zostaje wznowiona przez tę wysyłkę; ze starszymi czeka na Resume.
- Dispatch: pierwsza `queued`, gdy kolejka nie jest wstrzymana ani zablokowana
  (`attempting`/`delivery_unknown`) i sesja `ready_for_message` (Codex z celem: steer
  do tury celu). Najpierw `transition_runtime_queue(queued→attempting)`, potem w tle
  artefakty hosta pozycji (`artifact_ids`, obrazy wysłane zdalnie):
  `ArtifactStore::read_retained_image` → `attachments::prepare_image` do folderu
  załączników czatu, dane obrazów i `submit` pod ID wiadomości. To samo robią Steer
  i Edit pozycji, więc wiadomość z samym obrazem nie traci treści. Odmowa `Process::send`
  (limit 64 MiB/64 linii na proces i 256 MiB globalnie, zamknięte stdin) przed pierwszą
  linią = `failed`, po części linii = `delivery_unknown`. Sesja uznała już wiadomość za
  wysłaną, więc run kończy się (`abort_run`): kolejka zostaje wstrzymana, błąd mówi
  o Restarcie, goal i rozmowa zostają dla Restartu, nic nie jest ponawiane samo.
  `failed` wraca do composera z komunikatem.
- Rozstrzygnięcie tylko z `take_receipts`: `delivered`, `failed` albo `delivery_unknown`
  (także restart/zamknięcie runu bez dowodu). Sukces zapisu do stdin niczego nie dowodzi.
- Stop wstrzymuje kolejkę (`Pause`), Resume ją wznawia i najpierw wysyła `RESUME_PROMPT`
  (nietrwały prompt Canopy). `delivery_unknown` blokuje dalszy dispatch; tacka daje
  trzy akcje z bieżącą rewizją: Delivered, Send again (nowe ID w tym samym miejscu,
  ostrzeżenie o duplikacie), Dismiss. Grupa trzech przycisków ma jedną kartę na hover
  („Delivery unknown” z powodem i fakt na każdą akcję); same przyciski nie mają
  tooltipów, × zachowuje nazwę dostępności „Dismiss”. „Resume” w wierszu „Queue paused”
  też ma kartę („Resume queue”: jedna wiadomość na turę, praca przerwana Stopem
  kończy się pierwsza) zamiast jednoliniowego tooltipa.
- Move/Remove/Edit/Pause/Resolve to `QueueMutation` z rewizją (`change_queue`). Konflikt
  odświeża kolejkę i pokazuje komunikat; Edit z tacki zdejmuje pozycję do composera
  (Remove), a tekst z pola idzie jako nowa wiadomość.
- Steer (CHAT-03) to ta sama trwała wiadomość: ⌘⏎ z pola najpierw przyjmuje ją jak Send
  (ID, receipt), Steer pozycji zachowuje jej ID i komendę. Pozycja przechodzi na początek
  (`Move` z rewizją), dispatch zapisuje `attempting` i dopiero wtedy pisze ją w bieżącą
  turę (`ChatSession::steer_accepted`: Claude `priority: next` pod ID wiadomości, Codex
  `turn/steer`). Dowód: `started` Claude / odpowiedź Codex; odmowa = `failed` i powrót do
  pola, brak dowodu = `delivery_unknown`. Gdy tura skończy się przed wysłaniem, pozycja
  idzie jako zwykła następna wiadomość. Wstrzymana kolejka odmawia Steer z komunikatem.
  Agent bez capability `steer` (ACP) nie zamienia Steer w Send: szkic wraca do pola.
- API dla mostu gRPC: `ChatRuntime::accept_message(CommandInput, message, QueueDraft)`,
  `change_queue(CommandInput, revision, QueueMutation)`, `host_queue()` (rewizja, pauza,
  pozycje) i `queue_lines()`; wejście zachowuje DeviceId/epoch/ID komendy zdalnej.
  Odczyty: `ChatRuntime::snapshot` (status, run, opcje agenta, decyzje z runem, kolejka,
  obserwatorzy; bez transkryptu), `decisions()`, `Chats::chat_summaries`. Nazwane komendy
  (`runtime/remote.rs`, `chats/commands.rs`) przyjmują `CommandKey` urządzenia i typowane
  argumenty — bez uniwersalnego dispatchera: `remote_start/interrupt/restart/resume_queue/
  steer/respond/set_option`, `Chats::create_chat(NewChat)` (UUID od klienta, jawne konto,
  bez taba) i `delete_chat(DeletePreflight)`. Każda: accept (autoryzacja, dedupe), stan
  bieżący sprawdzony ponownie (zmiana → `rejected`), `attempting` przed skutkiem, wynik
  `succeeded`/`failed`/`delivery_unknown`. Odpowiedź na decyzję innego runu = `stale_run`,
  już rozstrzygniętą = `already_resolved`; Delete kończy się po potwierdzonym cleanupie.
  `observe`/`unobserve` liczą zdalne subskrypcje; obserwator jak widok blokuje usypianie.
  Dalsze komendy: `PauseGoal/ResumeGoal/ClearGoal` (tylko z capability `goal`),
  `StopBackground{call}`, `Rename{title}`; `Chats::fork_chat(ForkChat)` rozgałęzia
  rozmowę w tym samym punkcie co lokalny Fork, jako nowy czat bez taba.
  - Edycja jako fork (`ForkReplacement`): ścieżki z wiadomości rozwiązuje cel forka i
    autoryzacja się nie poszerza. Fork do projektu: plik musi nazywać workspace tego
    projektu (grant `READ` na jego checkout, ten sam root co cel); plik bez workspace
    = odmowa. Fork prywatny: plik z workspace = odmowa (root ≠ `private`); plik bez
    workspace (i ścieżka skilla) to plik prywatnego folderu źródła — tylko gdy źródło
    jest prywatne i ma folder (kanonizowany na workerze, `source_folder`); źródło
    projektowe nie ma prywatnego folderu, więc taki plik = `failed_precondition`.
  - Prywatny fork dostaje własne kopie: na background executorze, po utworzeniu
    nowego folderu `<dane>/chats/<uuid>` i przed atomowym `complete_runtime_fork`,
    `commands/fork_files.rs` kopiuje WYŁĄCZNIE dołączone pliki/katalogi (nie cały cwd)
    w te same względne miejsca i przepisuje ścieżki wiadomości na kopie. Folder
    źródła musi kanonizować się tak samo jak przy walidacji (inaczej
    `conditions_changed`). Kopia idzie bez podążania za linkami (Unix: `fstatat`
    + `openat` z `O_NOFOLLOW`, plik `O_NONBLOCK`, ponowny `fstat`), tylko zwykłe
    pliki i katalogi; link, FIFO, urządzenie, ścieżka spoza folderu źródła albo
    przekroczenie limitu (64 MB treści, 1000 wpisów, 32 poziomy) kończy fork jako
    `failed` bez rekordu i wiadomości, a nowy folder jest usuwany. Ścieżki sortowane
    i deduplikowane: plik wewnątrz już skopiowanego katalogu nie jest kopiowany
    drugi raz. Usunięcie źródła nie zabiera forkowi kontekstu zaakceptowanej
    wiadomości. Windows: bez `openat`, każdy krok sprawdza `symlink_metadata`.
- Szew serwera: `app_state::chat_bridge::install(cx) -> ChatBridge` (Send + Sync).
  Każda operacja idzie ograniczonym kanałem (64) do wątku aplikacji; pełny kanał =
  `resource_exhausted`, bez kolejki bez limitu. `subscribe(chat)` daje najpierw jeden
  `ChatEvent::Snapshot` (stan + najnowsze `SNAPSHOT_ITEMS`=100 elementów wszystkich
  części, `epochs_tail`, i znacznik starszych) z tej samej tury aplikacji co
  rejestracja, potem `Items{serial}`, `State`, `Committed` (dowód zapisu historii z
  `ChatHistory::HistoryCommitted`, dopiero po commicie), `SessionReplaced`, `Stopping`.
  Pełny bufor (256) kończy subskrypcję (klient robi resync), a runtime działa dalej.
  Subskrypcja liczy się jako jeden obserwator; `set_observers(id, n)` ustawia liczbę
  klientów, za których stoi (współdzielona projekcja: rzeczywista liczba, 0 w czasie
  oczekiwania na reconnect, więc czat może zasnąć). `watch_catalog` odświeża też
  po zmianach terminali i niesie rekordy katalogu (root uprawnień, konto, epoka).
  Bridge nie zna sieci, uwierzytelnienia ani Protobuf.
  `steer(chat, input, message, draft, run)`: Steer nowego tekstu to jedna komenda Send
  (`QueueDraft::command_for_steer`, intencja i run w fingerprint); inny run = `stale_run`
  bez przyjęcia, zmiana runu w trakcie przyjęcia wstrzymuje kolejkę zamiast steerować
  w inny run. Trwała wskazówka `steer_run` pozycji jest czyszczona przy starcie hosta.
  Interrupt, Restart i StopBackground niosą `run` i sprawdzają go w efekcie (w wątku
  aplikacji, po `attempting`), nie tylko przed wysłaniem. Run ID pochodzą z licznika
  procesu zasianego zegarem, więc run poprzedniego procesu Canopy nigdy nie pasuje.
  Katalog: `workspaces()` (projekty, kanoniczne rooty liczone na workerze, repozytorium),
  `watch_catalog()` (cały katalog projektów i czatów od razu, potem po każdej zmianie
  projektów, rejestru czatów lub rekordów — scalane, bez pollingu i bez tworzenia
  runtime'ów), `open_project` (bez zmiany wybranego projektu hosta),
  `close_project_preflight` / `close_project` (ponowne sprawdzenie, niezapisane pliki
  edytora hosta = `needs_host_interaction`, sukces po cleanupie czatów).
- Zdalne usługi (`app_state/remote/chat*`, `catalog*`):
  - Jedna projekcja na obserwowany czat, wspólna dla klientów, z jednym strumieniem
    `ReplayHub`. Snapshot i rejestracja ogona pod tą samą blokadą; ramki publikowane
    pod nią (SYNC-01). Ostatni klient → `set_observers(0)` i 30 s łaski: reconnect
    z kursorem dostaje brakujące ramki z ringu; potem projekcja i subskrypcja runtime'u
    się kończą. `SessionReplaced`/`Stopping` zamyka strumień (resync). Maks. 64
    projekcje (bezczynna zwalniana pierwsza), 100 elementów każda, cache obrazów
    ograniczony do używanych. Nieudany start projekcji wypisuje subskrypcję runtime'u
    także przy anulowanym RPC.
  - Snapshot i strona historii: elementy ≤512 KiB pakowane do jednej ramki, większe
    jako fragmenty ≤64 KiB tworzone leniwie z jednego współdzielonego kodowania
    (limit elementu 32 MiB, kursor dopiero po złożeniu). Przyjęcie transferów: 4 na
    urządzenie, 32 na hosta (`resource_exhausted`). Element live >4 MiB nie trafia do
    ringu (wypchnąłby inne strumienie): klienci dostają `Resync` i czytają snapshot.
  - Trwałość (SYNC-06/07): każda ramka elementu jest zapisana dopiero, gdy commit
    historii poda SHA-256 `history::item_json` dokładnie tej wersji; starsze ramki
    elementu stają się zapisane dopiero z dowodem nowszej wersji (samo zastąpienie
    niczego nie zapisuje). `saved_sequence` to najwyższa ciągła zapisana ramka;
    `TurnEnd.saved` tylko po dowodzie; elementy początkowe sprawdza
    `read_history_checkpoint`. Bez dowodu (historia wyłączona, błąd dysku) znak nie
    rośnie. Skrócony zapis ma ten sam hash co jego wersja live; strona historii niesie
    `trimmed` z zapisanego wiersza.
  - Załącznik ścieżki: względem checkoutu, a w Private chats (workspace pusty) względem
    prywatnego folderu czatu; ścieżka poza nimi ma tylko etykietę i pustą ścieżkę
    (nie do otwarcia, nigdy ścieżka klienta). Klient może wysłać plik prywatnego
    folderu jako `FileRef` bez workspace.
  - Fork (CHAT-08): historia do wskazanego końca tury jest kopiowana przez właściciela
    historii, potem jedna transakcja `complete_runtime_fork` publikuje rekord, odziedziczone
    referencje obrazów, opcjonalną edytowaną wiadomość (`replacement`, edycja jako fork)
    i sukces komendy. Wiadomość jest sprawdzana względem celu forka przed czymkolwiek
    i idzie pod komendą Send wyprowadzoną z forka (to samo urządzenie i epoka, ID
    `resource_id(key, "fork-message")`, fingerprint z fingerprintu forka zapisany w jego
    intencji). Błąd w tym miejscu = komenda `failed`, brak forka, wiadomości i
    częściowego wyniku (bez adnotacji w odpowiedzi); skopiowana historia nielistowanego
    czatu idzie z następnym cleanupem, prywatny folder jest usuwany od razu. Czat z
    wyłączoną historią nie ma punktu do forka (`Rejected`: brak zapisanego punktu).
  - Deduplikacja (CMD-02): każda mutacja czatu i katalogu liczy `wire_fingerprint`
    (SHA-256 zdekodowanego żądania bez transportowej tożsamości komendy — host, ID,
    epoka — z `expected_revision`, która jest częścią intencji) i sprawdza
    `get_runtime_command_matching` przed szukaniem celu. To samo ID z innym body,
    metodą, celem lub rewizją = `idempotency_conflict`, nigdy stary receipt. Handler
    runtime'u (`RemoteKey{key, fingerprint}`) zapisuje komendę pod tym samym
    fingerprintem, niezależnie od tego, jak cele rozwiązują się później.
  - Edycja wysłanej wiadomości (`PreviewEdit`/`EditMessage`): podgląd pyta runtime
    (`remote_edit_preview`, dry run plików agenta ≤10 s) i zwraca oryginał jako
    `MessageDraft` (obrazy opublikowane, ścieżki względne, PR podpisane ponownie);
    powód odmowy bez prefiksu kodu. `EditMessage` = `ChatCommand::Edit` (opcjonalne
    przywrócenie plików przed edycją; sukces po starcie nowej gałęzi).
  - Komendy `!` (`RunShell`/`StopShell`, zakres chat+terminal): `ChatCommand::RunShell`
    z runem; receipt sukcesu niesie klucz karty (`history_epoch`, `item_id`) jako
    wynik `chat.shell`. StopShell (`run`, `history_epoch`, `item`) zatrzymuje kartę
    danego runu i części historii: w trwałym skutku `stale_run` przy innym runie,
    `conditions_changed` przy innej części (ten sam numer elementu może być inną
    kartą bieżącej części), dopiero potem sprawdzenie, czy karta jeszcze działa.
  - Załącznik PR: tylko z podpisanym `context_id` z listy PR hosta (`PullContexts`,
    HMAC kanonicznych pól i roota checkoutu, 15 min). Brak kontekstu =
    `pr_context_required`; zmienione pola, inny checkout lub wygaśnięcie = odmowa.
    `ChatsRpc::with_pull_contexts` dostaje ten sam signer co discovery.
  - Katalog: lista i `WatchSummaries` filtrowane rootami urządzenia; najpierw cały
    katalog, potem delty czatów/terminali, ponownie cały przy zmianie projektów.
    Budzi go push aplikacji, zmiana dzierżawy terminala (`Run::observe`) i stan
    hosta (draining kończy strumień). Credential urządzenia jest sprawdzany przed
    KAŻDYM wysłanym zdarzeniem; zmiana autoryzacji (root, scope, rewizja) usuwa
    wszystko zakolejkowane dla starej i wysyła nowy snapshot, revoke kończy strumień
    przed kolejnym zdarzeniem. Katalog > 64 KiB idzie jako `CatalogSnapshotPart`
    (strony ~64 KiB, wspólny `boundary`, `first`/`last`; projekt może kontynuować
    checkouty na kolejnej stronie pod tym samym `project_id`); mniejszy jako zwykły
    `snapshot`. Klient (`remote::client::catalog`, mobile `grpc/catalog.ts`) stage'uje
    strony i podmienia katalog atomowo dopiero na `last` z tym samym boundary; delta
    w trakcie, obcy boundary lub rozłączenie odrzuca staging (stary katalog zostaje).
    `ReadCatalog` = jeden taki ciąg stron (używany przy connect i przez outbox);
    unarne `ListResources` zwraca `resource_exhausted` (`catalog_too_large`) powyżej
    1 MiB. Projekcja jest liczona do wspólnego budżetu `ReplayHub::reserve_assembly`
    (32 MiB na host, inaczej `resource_exhausted`), wysłane zdarzenie do
    `reserve_output` urządzenia (4 MiB/32 MiB) do czasu pobrania następnego; po
    wysłaniu strumień trzyma tylko skróty wierszy. Most aplikacji trzyma jeden
    współdzielony `Arc<CatalogEvent>` na strumień (najnowszy, bez kolejki wersji) i
    nie buduje ani nie wysyła katalogu, gdy projekty, podsumowania czatów i rekordy
    się nie zmieniły (np. token tekstu). Mutacje:
    CreateChat (ID z komendy, jawne konto), Rename (tylko czaty; projekty, terminale
    itd. nie mają rename w Canopy → `unsupported`), OpenProject, PreflightClose i
    CloseResource (czat = Delete, projekt = Close, terminal urządzenia). Plan
    zamknięcia jest związany z urządzeniem, zasobem i zamiarem, ważny 5 min, zużywany
    przez jedną komendę; host sprawdza warunki (i dirty presence projektu) ponownie.
    Retry tego samego ID zwraca zapisany stan przed sprawdzeniem celu, więc po
    usunięciu celu lub zużyciu planu nie ma drugiego skutku ani mylącego not_found;
    inna operacja, cel lub body (fingerprint) pod tym ID to `idempotency_conflict`.
    Close projektu, którego cleanup czatów się nie udał, kończy się `failed`
    (`cleanup_failed`), a projekt zostaje na liście jako `closing` (bez wejścia do
    katalogu projektów i terminali), z korzeniem uprawnień do czasu udanego cleanupu.
    Ponowny PreflightClose/CloseResource tego workspace'u (nowa komenda) ponawia tylko
    nieudane cleanupy tych czatów; sukces usuwa go z listy. `WorkspaceRoots` śledzi
    kanoniczne workspace'y z pushy katalogu (zadanie startuje przy pierwszym RPC
    katalogu, kończy się z usługą).
- Autoryzacja komend czatu używa `ChatRecord::authority_root` (kanoniczny checkout
  albo "private"), ustawianego przy zapisie rekordu, create, fork i recover poza
  wątkiem UI; pusty root odmawia, nie jest zgadywany.

Usługi hosta (`runtime/host_services.rs`) są ogłaszane w `initialize`:
`fs/read_text_file` czyta bufor otwartego edytora (z niezapisanymi zmianami),
a bez niego plik przez `files::Document` (limity, UTF-8, brak linków poza
folderem). `fs/write_text_file` odmawia, gdy plik ma niezapisane zmiany w
edytorze Canopy albo zapisany dirty buffer innego urządzenia (`runtime_file_presence`,
komunikat z nazwą urządzenia i last_seen), i zapisuje przez `Document::save` (BOM,
CRLF, uprawnienia, odmowa przy zmianie na dysku); nowy plik powstaje w istniejącym
katalogu. Zapisy Canopy jednego pliku (Save edytora, zapis agenta, zdalne Files/Git)
rezerwują wspólną bramkę hosta `files::Gate` (globalna `app_state::file_ops`, ta sama
dla serwera przez `ResourceLocks::with_gate`) po kanonicznym root i ścieżce; zapis
agenta po rezerwacji ponownie sprawdza oba źródła dirty tuż przed zapisem. To nie
jest CAS wobec innych programów.
Ścieżki muszą być absolutne i leżeć w folderze czatu. `terminal/*` uruchamia komendę
przez `shell::Running` w folderze czatu ze środowiskiem powłoki (macOS/Linux: powłoka
i grupa procesów; Windows: program i argv bezpośrednio przez `prepare_launch`,
uruchomiony wstrzymany w nowym Job Object, więc Stop i cleanup obejmują całe drzewo),
limit 16 terminali i
1 MiB outputu (ucinany od początku). Terminal należy do runu: Stop, wyjście
procesu i `release` go kończą, a uchwyt trafia do wspólnego cleanupu. Karta
narzędzia pokazuje jego output na żywo, bez wejścia i resize. Wyniki spóźnione
po restarcie są odrzucane po generacji runu.
Output terminala jest treścią disclosure narzędzia (`measured_body`, ten sam szewron,
PANEL + CONTENT_REVEAL i `AnimatedHeight` przy dopisywaniu), w polu `tools::output_box`
w kolumnie tekstu jak treści kart. Wiersz otwiera się sam, póki terminal działa
(w nagłówku dopisek „Agent terminal”), i zwija po jego wyjściu
(`ChatView::follow_agent_terminals` przy zmianie runtime'u, `Rows::follow_terminal`).
Ręczne przełączenie (`touched`) wygrywa: wiersz zostaje taki, jak go zostawiono.
Po wyjściu nagłówek pokazuje tylko status wywołania (✓/✕ według stanu narzędzia od
agenta); podsumowania outputu nie wyciągamy. Kod wyjścia nie jest dziś udostępniany
z `HostTerminals` do UI. Zwolniony terminal (`release`, koniec runu) znika z karty.

LIFE-09: runtime bez żadnego `ChatView` i bez zdalnego obserwatora (także ukryty
tab ma widok, więc nie usypia) po 60 s bez tury, decyzji, aktywnego dispatchu (wstrzymana kolejka i
`delivery_unknown` nie blokują), `!`, terminali, pracy w tle i
goalu — i tylko z zapisanym resume — zatrzymuje proces i trzyma rozmowę jako
zapisaną część; kolejny start wznawia tę samą sesję w tej samej części historii.
Globalny limit `MAX_CHAT_PROCESSES` (32) odmawia startu z komunikatem błędu i
Restartem zamiast tworzyć kolejny proces. Sidebar i taby czytają status i model
z runtime'u, nie z widoku.

Rozmowy adoptowane bez metadanych (`unresolved`) są wymienione w sidebarze czatów
z akcją „Restore chat…”. Dialog (`ui/chat_recovery.rs`) wymaga wyboru agenta, konta
i folderu (nowy prywatny albo otwarty projekt); podpowiada tylko konto zapisane w
historii tej rozmowy, nigdy domyślne. `Chats::recover` zapisuje rekord jawnym CAS
(`recover_runtime_chat`: ten sam ID, powód i `history_epoch`); historia i ID zostają,
a czat pojawia się jak czat bez taba. Konflikt lub nieznane konto zostawia rekord.

Ograniczenia: brak testu z rzeczywistym agentem ACP i GUI; terminal ACP i `!` na
Windows nie były uruchomione (brak hosta Windows: NOT RUN).

### Zdalny katalog composera i strony

`remote::discovery` odczytuje niesekretne definicje i konta z rzeczywistego
`ToolCatalog` z zakresem `read`. Lista ma strony ograniczone bajtowo do control;
kursor wiąże pozycję z rewizją publicznego katalogu. Zmiana katalogu wymaga
ponownego odczytu pierwszej strony. Konto bez profili używa jawnego `tool.id`;
ścieżki uruchamiania native i ACP interpretują je jednakowo, bez przełączenia
na nowy domyślny profil, jeżeli katalog zmieni się przed uruchomieniem.

Sonda wybranego konta uruchamia handshake bez promptu. Codex czeka na odpowiedzi
modeli i skilli, ACP na aktualizację komend; brak kompletu metadanych po 15 s daje
jawny stan niepełnego katalogu. Wybrany profil zachowuje model, tryb i effort.
Odświeżanie nie zajmuje właściciela aktywnego czatu. Maksymalnie cztery sondy
pracują równolegle; anulowanie RPC kończy sondę, a wspólny drain czeka na cleanup.
Cache jest związany z rootem, agentem, kontem i konfiguracją; także trafienie
w cache ponownie sprawdza aktualny grant.

Odczyt PR sprawdza `read`, bieżące repozytorium oraz niezmienione konto przy
każdym wysłaniu HTTP. Kontekst PR ma podpis związany z checkoutem i pełną treścią,
ważny przez 15 minut. Nie jest samodzielnym uprawnieniem do zasobu.
`ReadChatPage` odczytuje jeden rekord czatu i jeden wskazany element historii
(albo element live), odrzucając skrócony zapis HTML. Obrazy względne i bezwzględne
muszą pozostać po kanonikalizacji wewnątrz folderu czatu; folder czatu w projekcie
musi należeć do autoryzowanego workspace. Inlining ma łączny limit 32 MiB i
ogranicza odczyt pliku przed alokacją base64. Nieudana publikacja cofa upload;
opublikowany wynik podlega limitowi i TTL magazynu artefaktów. Każdy odczyt
fragmentu ponownie sprawdza aktualny grant.

Regresje izolowane używają procesów fixture, tymczasowych repozytoriów i SQLite.
Nie stanowią odbioru rzeczywistych providerów, GUI ani platform macOS/Windows/mobile;
te próby pozostają `NOT RUN` do osobnego wykonania.

### Zdalny szkic podczas wysyłania

Desktop i mobile przechwytują wersję całego composera przed wysyłką: tekst,
obrazy oraz załączniki i `SendContext`. Każda edycja, także zmiana samego obrazu
lub cytatu przy identycznym tekście, tworzy nową wersję. Zapis do outboxu atomowo
usuwa tylko przechwyconą wersję szkicu. Kontekstowy Send jest dostępny online;
po przyjęciu czyści szkic przez porównanie wersji. Nowsza edycja pozostaje
w kontrolce i prywatnym magazynie klienta, także po ponownym otwarciu rozmowy.
Programowe `InputState::set_value` nie emituje `Change`, dlatego desktop jawnie
zapisuje tekst i kontekst razem oraz odrzuca opóźnione autosave zużytej wersji.

Kontekstowy Send ma typowany dziennik oryginalnego żądania i używa tego samego
command ID przy jawnym uzgadnianiu utraconej odpowiedzi. Outbox offline nadal
przyjmuje tylko tekst i obrazy. Czyszczenie szkicu nie usuwa dzienników uploadu
ani plików obrazów potrzebnych przyjętej wiadomości. Cancel edit najpierw
przywraca cały wcześniejszy composer, potem zapisuje przywrócony stan.
Zakończenie Shell/Edit korzysta z tej samej ochrony wersji. Uzgodnienie dawnego
Edit po utracie odpowiedzi zachowuje bieżący szkic; Cancel edit przywraca wcześniejszy.

Regresje magazynu klienta używają barier i tymczasowych danych: A wysyłane,
B edytowane przed wynikiem A, odtworzenie B po restarcie, brak odtworzenia
zużytej wersji oraz błąd zapisu bez utraty szkicu. Te testy nie potwierdzają GUI
ani odbioru natywnego macOS, Windows, iOS i Android — pozostaje `NOT RUN`.

## Jeden widok rozmowy na wszystkich komputerach

Lokalne i zdalne rozmowy korzystają z `ui::chat::ChatView`, wspólnego composera,
rendererów wiadomości i narzędzi, zgód, kolejki, celów oraz wyszukiwania. Adapter
`remote::client::presentation` zachowuje semantykę narzędzi i rozdziela identyfikatory
wywołań według epoki. Klientowy wariant `ChatRuntime` subskrybuje hosta; nie tworzy
lokalnego procesu agenta ani lokalnej historii tej rozmowy.

`PaneMetadata.remote` zawiera wyłącznie UUID hosta i czatu. Zwykłe taby, przeciąganie,
splity i odtwarzanie sesji są własnością urządzenia. Ten sam czat można oglądać w
kilku pane'ach i na kilku komputerach. Tab podaje nazwę komputera; jego zamknięcie
odłącza prezentację. Dotyczy to również lokalnego taba hosta: Delete chat pozostaje
odrębną operacją z dotychczasowym potwierdzeniem i cleanupem.

Zdalny composer zapisuje szkic z rewizją w prywatnym magazynie klienta; zakończenie
wysyłania nie czyści późniejszych zmian. Konta i opcje pochodzą z katalogu właściwego
hosta. `SetChatAgent` zmienia jawnie wybrane konto tylko przed pierwszą wiadomością.
Steer z `draft` wysyła nowy tekst; bez `draft` wskazuje istniejący `message_id` kolejki.
Oba warianty sprawdzają generację uruchomienia. Wiadomość zapisaną wyłącznie na
kliencie można przestawić w jego kolejce lub przenieść do composera; przeniesienie
utrwala szkic razem z usunięciem wpisu kolejki. Wiadomość, której wysyłkę rozpoczęto,
wymaga wcześniej uzgodnienia wyniku. Wpis potwierdzony w kolejce hosta nie jest
pokazywany drugi raz jako lokalna wiadomość o nieznanym wyniku.
Obrazy są pobierane jako zweryfikowane artefakty, pliki `@` i PR-y `#` zachowują zakres
hosta, a HTML otrzymuje autoryzowane bajty w izolowanym podglądzie. Historia dochodzi
stronami; wyszukiwanie korzysta z indeksu hosta. Kliknięcie pliku na karcie narzędzia
otwiera plik właściwego hosta w bocznym panelu, bez odłączania widoku rozmowy.
Taki plik ma wygląd lokalnej pigułki (`file_pill::look`: ikona typu, nazwa skracana
w kontenerze `flex_1/min_w_0`, liczniki i status zostają w wierszu); tooltip podaje
pełną ścieżkę i nazwę hosta, a kliknięcie emituje `ClientFileRequested`.

Żądanie zapisane na urządzeniu (Send, Edit, Fork, `!`, zatrzymanie polecenia), którego wyniku
host nie potwierdził, pokazuje tacka nad composerem w miejscu kolejki (`remote::recovery_bar`, gdy nic nie
jest edytowane): kropka i zdanie według `CommandReceipt.state` — brak potwierdzenia „Message not confirmed
by {host}” (żółta), ACCEPTED/ATTEMPTING „{host} is processing it” (akcent), DELIVERY_UNKNOWN „{host}
couldn't confirm it” (żółta), NOT_FOUND „{host} has no record of it” (czerwona). Pod nim treść żądania
(z `ChatMutation`), pełna w karcie na hover ze statusem. Akcje jak w kolejce: „Check status”
(`client_reconcile`) i „Send again” (`client_retry_saved`, tylko bez potwierdzenia albo przy NOT_FOUND).
`RemoteSession.recovering` trzyma trwającą akcję: jej przycisk ma spinner (`ButtonLoading`), Send again
znika na czas sprawdzania. Przycisk „Earlier messages” jest kryjący (`loading_button`, `shadow_md`), ze
spinnerem podczas pobierania strony; błąd strony (`RemoteSession.history_error`) zamienia go w „Couldn't
load · Retry” z ikoną ostrzeżenia. Plik spoza obrazów dodany w zdalnym czacie daje błąd pod polem: „{host}
can't open {plik} from this computer. Type @ to choose a file on {host}, or attach an image.” (dla kilku
plików „N files”).

Sprawdzenia jednostkowe obejmują roundtrip tabów/splitów, rozdzielenie hostów,
aktualizowanie wspólnego transkryptu oraz zachowanie kolejki po odłączeniu taba.
Nie zastępują odbioru GUI ani rzeczywistego połączenia między komputerami.
