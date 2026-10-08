# Kontrakty produktu i implementacji Canopy

Szczegółowe kontrakty przeniesione z głównego `AGENTS.md`. Czytaj tylko sekcje
dotyczące zmienianego obszaru; odnośniki do opisów modułów są przy odpowiednich
regułach. Ścieżki kodu są względem korzenia repozytorium. Kontrakty zachowuj,
chyba że użytkownik je zmienia. Wersje i stan implementacji sprawdzaj w kodzie
i Cargo.lock; historyczne wyniki w docs/ nie dowodzą obecnego stanu.
Zasady pracy: [AGENTS.md](../AGENTS.md). Dobór testów: [testing.md](testing.md).

## Cel i sposób rozwijania

- Rozwijamy Canopy w Rust + GPUI Kit w tym repozytorium, na branchu `rust-rewrite`.
  Referencja Electrona pozostaje w historii brancha `next`, a screeny w
  `ref-images/` i materiałach dostarczanych przez użytkownika.
- Katalog `mobile/` zawiera niezależną aplikację Expo/React Native zachowaną
  z `next`. Jej zależności i polecenia npm należą wyłącznie do tego katalogu.
  Dokumentacja `docs/features/remote-control.md` opisuje protokół Electrona,
  nie działającą integrację mobile z wersją Rust. Szczegóły przeniesienia:
  [rust-rewrite.md](rust-rewrite.md).
- Wygląd ma odtwarzać Electron możliwie 1:1: kolory, alfa, typografia, odstępy,
  gęstość, wcięcia, ikony, stany interakcji. Nie wykonuj samowolnego redesignu.
- Zachowanie wskazane później przez użytkownika ma pierwszeństwo przed
  wcześniejszym screenem. Przykłady: nagłówki i worktree bez tła, puste nowe
  workspace'y, Preferences w osobnym oknie.
- Electron jest źródłem referencji produktu, nie obowiązkowej architektury.
  Nie kopiuj jego IPC, store'ów ani słabych zależności do Rusta.
- Rozwijaj funkcje całościowo: stan, operacje, UI, błędy, persist/restore,
  cykl życia i adekwatna weryfikacja. Domykamy etap przed kolejnym.
- Korzystaj z istniejących komponentów. Rozbudowuj moduły według rzeczywistych
  potrzeb; unikaj zarówno monolitycznych renderów, jak i abstrakcji bez użycia.
- Stan początkowy nie może zawierać fixture udających projekty, pliki ani procesy.
  Nie przedstawiaj pozostających mocków jako działających integracji.

## Stos, źródła i granice

- Używaj fasady `gpui_kit`. Nie dodawaj niezależnej wersji GPUI ani innego źródła
  frameworka. Obecna baza: GPUI Kit 0.7.1 / gpui-pre 0.3.8, Rust 1.95.0.
- Zachowuj `rust-toolchain.toml`, przypięte wersje i Cargo.lock. API sprawdzaj
  w źródłach wersji rozwiązanych przez Cargo, a nie przez analogię z React/CSS.
- Celową zmianę zależności uzasadnij; potem wykonuj Cargo z `--locked`.
- Aplikacja używa `git2` z vendored libgit2, nie subprocessów `git` ani shella.
  To ograniczenie kodu aplikacji; polecenia git do pracy nad repozytorium
  (status, diff, commit) są normalnym narzędziem developerskim.
- Alacritty Terminal odpowiada za emulator ANSI, grid i PTY; GPUI rysuje widok.
  Nie osadzamy okna zewnętrznego emulatora terminala.
- Główna kwalifikacja platformowa to macOS. Nie deklaruj sprawdzenia innych
  platform, wszystkich Spaces/monitorów ani fizycznych 120 FPS bez dowodów.

## Design tokeny

- Kolory, typografia i odstępy mają źródło w `src/ui/theme.rs`. Zachowuj wartości
  i przezroczystość tokenów Electrona; nie zaokrąglaj konwersji OKLCH do 8-bit
  w pośrednich krokach ani nie zastępuj kolorów wizualnym przybliżeniem.
- Podstawowa jednostka spacingu to 4; obecne wysokości: wiersz 28, tab strip 32,
  status bar 24 jednostki logiczne. Przed zmianą sprawdź stałe w kodzie.
- Używaj semantycznych bg/sidebar/elevated/text/secondary/muted/faint/border,
  zamiast rozsiewać lokalne heksy. Brak tła w nagłówkach nie oznacza usuwania
  poprawnego selected background w FileTree czy innych kontrolkach.
- Zachowuj właściwy font tekstu interfejsu oraz osobny font monospace terminala.
  Nie skaluj całego UI w celu naprawienia pojedynczej ikonki lub odstępu.

## Własność stanu i moduły

`src/app_state.rs` rejestruje globalny `AppState` jako zbiór uchwytów do
oddzielnych encji. Nie rób jednego globalnego obiektu odrysowującego całe UI.

| Właściciel | Odpowiedzialność |
| --- | --- |
| `SettingsState` | zatwierdzone preferencje, ładowanie/zapis, błędy, klient SQLite |
| `ProjectsState` | katalog otwartych folderów/worktree, aktywny WorkspaceId, picker, cache nieaktywnych workspace'ów, persist/restore |
| `state::workspace::Workspace` | uporządkowane taby, aktywny tab, drzewa splitów, fokus pane'a |
| `LayoutState` | szerokości/widoczność sidebarów i wybór zakładki inspektora |
| `ToolsState` | zatwierdzony katalog narzędzi/profili, wykrywanie executable i środowiska |
| `Terminals` | runtime PaneId → TerminalView, lazy start, statusy, zadania stop/cleanup |
| `GitState` + `git/service.rs` | cache metadanych, worker libgit2, watchery, operacje worktree |
| `FilesState` / `Editors` | cache rozwiniętych katalogów aktywnego worktree, runtime edytorów po PaneId, zapis i ochrona niezapisanych zmian |
| `ChangesState` / `Diffs` | aktywny snapshot staged/unstaged, operacje i commit, runtime widoków diffu po PaneId |
| `IntegrationsState` | konta/Keychain, odczyt Tasks, cache i trwałe powiązania worktree z zadaniami |
| `AgentsState` | runtime sesji agentów, odbiornik hooków, powiązanie run → pane, dane inspektora/notcha |
| encje widoków | hover, motion, drafty formularzy, focus, uchwyty okien i subskrypcje |

- Logika layoutu w `src/state/` jest oddzielona od renderowania i SQLite.
  Widoki tłumaczą gesty na operacje domenowe, a po zmianie wywołują notify.
- W obecnym modelu wpis `Project` identyfikuje otwarty katalog/worktree:
  `path` to konkretny katalog, `repository_path` grupuje wpisy jednego repozytorium
  w sidebarze. Nie zakładaj, że jedno repozytorium oznacza jeden WorkspaceId.
- Model aktywnego workspace'u znajduje się w globalnej encji Workspace;
  nieaktywne modele zachowuje ProjectsState. Przełączenie nie miesza ich tabów.
- InputState, SelectState, TextareaState, focus, Subscription i Task mają
  jawnego właściciela. Twórz je przy inicjalizacji lub obsłudze zmiany, nie w renderze.
- Render nie wykonuje I/O, SQL, odczytu katalogów/Git, parsowania konfiguracji
  ani tworzenia procesów. Nie uruchamiaj w nim pętli notify lub trwałych workerów.
- Zadania async muszą przeżyć do końca operacji albo mieć kontrolowane anulowanie.
  Odrzucaj spóźnione wyniki generacją/tożsamością. Nie gub cleanup przy zamknięciu
  taba, pane'a, projektu, okna ani podczas wychodzenia z aplikacji.

## Layout i okna

- Lewy sidebar zawiera Projects, Files i Tools. Główny obszar zawiera pasek tabów
  i pane'y. Po prawej jest opcjonalny inspector/Changes. Na dole status bar
  z ustawieniami; przełączniki sidebarów są w titlebarze.
- Preferences otwieramy jako osobne okno aplikacji, nie modal całego workspace'u.
  Mniejsze formularze/potwierdzenia korzystają ze wspólnego systemu modali.
- Każde okno ma Root i właściwe warstwy dialogów, popupów, tooltipów itd.
  GPUI Kit 0.7 montuje warstwy komponentów automatycznie w Root; nie dodawaj
  ręcznych wywołań renderowania tych warstw w widokach aplikacji.
- Oba sidebary mają zachowywać szerokość ustawioną przez użytkownika podczas
  resize okna. Przestrzeń między nimi wypełnia terminal; nie dziel szerokości
  proporcjonalnie między terminal a inspector.
- Maksymalna szerokość każdego sidebara to 800 jednostek logicznych. Gdy okno
  jest za wąskie, dopasuj jedynie wyświetlane szerokości, zachowując zapisane
  preferencje i co najmniej 160 dla głównego obszaru. Ukryty panel nie zajmuje miejsca.
- Toggle sidebarów nie może psuć ich separatorów resize ani obszarów chwytania.
- Przy zwijaniu obu sidebarów ich zawartość zostaje zakotwiczona; wizualnie
  terminal przykrywa/odsłania panel. Prawy sidebar nie ma wyjeżdżać całością poza okno.
- Prawy sidebar używa tego samego koloru powierzchni co lewy.
- Toggle prawego sidebara jest po prawej stronie titlebara, analogicznie do lewego.
  Nie pokazujemy już oczka w dolnym pasku; ustawienia pozostają na dole.
- Używaj wspólnego `window_titlebar`; zachowuj natywne działanie podwójnego
  kliknięcia górnego paska zgodnie z ustawieniem macOS.

## Projekty, worktree i puste stany

- Bez wybranego projektu: centralne Open folder, bez layoutu sidebarów/tabów.
- Nowy projekt lub worktree ma pustą listę tabów. Nie twórz automatycznie
  taba, pane'a, shella ani agenta podczas otwierania/przełączania worktree.
- Przy wybranym workspace bez tabów centralny CTA `Open shell` otwiera jawnie
  Shell w jego cwd. Nie używa przypadkowego domyślnego agenta zamiast Shell.
  CTA znika po pierwszym tabie i wraca po zamknięciu ostatniego.
- Pusta lista tabów jest poprawnym stanem i przeżywa persist/restore.
- Przełączenie worktree aktualizuje zaznaczenie poprzedniego/nowego wiersza
  oraz odpowiedni zestaw tabów/pane'ów. Nie animuje wejścia całego layoutu
  ani sidebarów. To samo dotyczy przełączania już otwartych projektów.
- Nie usuwaj zapisanych tabów tylko dlatego, że zmieniła się polityka tworzenia
  nowych workspace'ów. Zachowuj dane użytkownika i niedostępne zapisane katalogi.

## Sidebar: wygląd i interakcje

- Projects, Files i Tools są zwijalne. Ich nagłówki zmieniają kolor tekstu
  na hover; nie podświetlaj całego tła jak dużego przycisku.
- Projekt jest rozwijanym nagłówkiem: strzałka i nazwa tworzą jedną akcję,
  bez folderowej karty z tłem. Obok są `+ new` i zamknięcie projektu.
  Ręczny Refresh Git metadata jest w menu kontekstowym nagłówka.
- Worktree są wciętymi etykietami wyrównanymi do lewej, bez zaznaczenia tłem.
  Hover rozjaśnia tekst; aktywny katalog ma gwiazdkę. Usuwanie pojawia się na hover.
- Narzędzie z ponad jednym profilem ma wspólny nagłówek strzałka/ikona/nazwa:
  kliknięcie rozwija/zamyka profile, nie uruchamia procesu. Hover rozjaśnia
  nazwę i strzałkę; ikona marki zachowuje właściwy kolor.
- Przy jednym profilu nie pokazuj strzałki ani pustego miejsca po niej:
  kliknięcie uruchamia ten profil. Przy braku profili uruchamia bazowe narzędzie.
- Nazwy narzędzi i profili są wyrównane do lewej. GPUI Button centruje własny
  kontener etykiety — samo ustawienie justify na zewnętrznym elemencie nie wystarcza.
- Czerwony Stop jest widoczny, gdy workspace ma procesy, również dla
  `Folder without Git`. Obejmuje wszystkie jego taby, nie wszystkie projekty aplikacji.
- Stop i usuwanie używają wspólnej geometrii: 16 × 16 jednostek logicznych,
  wyśrodkowany symbol, identyczne wyrównanie i zaokrąglenie. Stop ma czerwony
  środek, hover tła czerwony z alfą 0.12, pressed 0.20.
- Ustal również min/max width/height małych przycisków. `compact()` może
  narzucić minimalną szerokość większą niż `.size()`, tworząc prostokąt.
- Sidebar ma scrollbar. Zwijanie FileTree nie zmienia odstępów między nagłówkami.
  Selected/hover tła wierszy plików mają właściwe poziome odsunięcie od krawędzi.

## Komponenty i krytyczna pułapka hoveru

- Publiczne elementy prezentacyjne zwracają konkretne Div/Stateful/Button,
  żeby caller mógł dalej używać Styled oraz przypinać callbacki.
- Akcje w tle używają widokowego `ButtonLoading` oraz wspólnych helperów
  loading_button / primary_loading_button / loading_icon_button. Ustawiaj stan
  w handlerze/subskrypcji konkretnej operacji, nie na podstawie dowolnego disabled.
  Zachowuj animowany slot loadera, nie zmieniaj skokowo etykiety ani szerokości.
  Primary bierze kolory z wariantu i theme, żeby statyczne bg/foreground nie
  nadpisywały disabled. Backdrop modala blokuje hover, kliknięcia i scroll przez
  `occlude`, również w animacji wyjścia; focus pozostaje w aktywnym modalu.
- W GPUI odpowiednikiem `class`/łączenia klas jest builder Styled. Nie dodawaj
  parsera CSS tylko po to, żeby mechanicznie skopiować poradnik webowy.
- Marginesy między komponentami należą do call site. Wspólne wnętrze kontrolki
  może zawierać padding/gap. Korzystaj z istniejących input/dropdown/textarea/button.
- Wspólne wzorce: `form_field`, `stacked_setting`, `preference_section`,
  `SelectOption`, `ToolHeader`, `Disclosure`, `hover_action`, modal helpers.
- **GPUI Kit 0.6.0 managed tooltip instaluje własny on_hover na Button.**
  Może nadpisać poprzedni handler; w debug dwa listenery na jednej interaktywności
  są niedozwolone. Dla hoveru sterującego stanem używaj osobnego kontenera
  `hover_action`, a tooltip zostaw na przycisku wewnątrz. To naprawiona regresja.
- Kontener hoveru obejmuje cały logiczny cel, aby przejście z labelki na ikonę
  nie kasowało stanu. Nie twórz oddzielnego przycisku strzałki w jednym nagłówku.
- Stabilne ID elementów wynikają z modelu, nie z aktualnego indeksu/nazwy ani
  losowania w renderze. Hover, focus i otwarte formularze nie mogą przeskakiwać.

## Motion design

Źródła: `src/motion/`, `src/ui/theme.rs`, [motion.md](motion.md).
Motion traktujemy jak design tokeny, nie lokalne dekoracje.

| Sytuacja | Zasada |
| --- | --- |
| rozwijanie sekcji, projektu, profili, FileTree | wspólny Disclosure, animacja wymiaru i treści, spójna strzałka |
| pokazanie/schowanie sidebara | PANEL, stałe zakotwiczenie panelu i prawidłowy resize |
| modal, popup, potwierdzenie | POPOVER, animowane wejście i wyjście z zachowaniem focusu |
| przełączenie profilu w Preferences / treści inspektora | lokalne przejście treści |
| pusty ekran ↔ layout projektu | można użyć CONTENT_REVEAL dla wejścia odpowiedniego stanu |
| zmiana worktree w istniejącym layoucie | bez globalnego wejścia/fade/przesunięcia sidebarów |
| zwykły hover etykiety | rozjaśnienie tekstu, bez skali/przesuwania i tła |
| ciągły resize okna lub separatora | bez animowanego gonienia pozycji terminala |

- Używaj presets PANEL, POPOVER, CONTENT_REVEAL, RESIZE, STATE_CHANGE według
  znaczenia. Nietypowe sekwencje składaj ze wspólnych tokenów.
- Presence/Transition trzymamy w widoku. Cel zmieniamy w handlerze, czas/progress
  odczytujemy w renderze. Retarget zaczyna się od bieżącej wartości.
- Nie restartuj animacji przy identycznym celu ani przy każdej notyfikacji.
- `motion::policy(cx)` respektuje Reduce Motion. Brak stałego timera i pętli
  klatek w spoczynku; `motion::request_frame` tylko podczas aktywnej animacji.
- **`window.request_animation_frame()` wymaga kontekstu renderowania.**
  Wywołanie w handlerze myszy powodowało panic/crash. W handlerze używaj
  notify/refresh, a animacyjne żądanie klatki zostaw w renderze.
- Utrzymuj element przez animację wyjścia, potem odmontuj. Wyłącz akcje ukrytej
  lub zamykanej treści, aby niewidoczne przyciski nie przechwytywały kliknięć.
- Nie dopuść, żeby clipping animowanych kontenerów zaczął działać jak dodatkowy
  scroll. Testuj overscroll FileTree: elementy nie mogą znikać po kręceniu na granicy.

## Taby, pane'y i procesy

- Aktywny tab ma być widoczny w poziomym viewportcie; Cmd+T, aktywacja, zmiana
  kolejności i resize odsłaniają go minimalnym animowanym przewinięciem RESIZE.
  Ręczny scroll przerywa animację. Reduce Motion ustawia offset natychmiast.
- Każdy tab posiada własne binarne drzewo splitów. Liść to pane; split ma oś,
  proporcję i dzieci. Aktywny tab i focused PaneId to oddzielne stany.
- WorkspaceId, TabId, PaneId, SplitId są stabilne również po restore.
  Deserializacja rezerwuje ID w generatorach, by nie tworzyć kolizji.
- Pane header/handle pokazujemy dopiero przy więcej niż jednym pane w tabie.
- Drag/drop: krawędź pane'a tworzy split, środek zamienia pane'y; pane przeciągnięty
  na wolne miejsce tabów staje się tabem; tab może stać się częścią drzewa pane'ów.
  To przeniesienie istniejącego PaneId, nie tworzenie nowego procesu.
- Zamknięcie pane'a upraszcza drzewo; zamknięcie ostatniego pane'a zamyka tab.
- Tab ma menu kontekstowe zmiany nazwy i zamknięcia całego drzewa.
  Na hover pokazuje X po prawej. Kliknięcie X nie aktywuje taba i nie rozpoczyna drag.
  Środkowy przycisk myszy zamyka wskazany tab tą samą ścieżką co X, również nieaktywny.
- Proces uruchamiamy jako konkretny executable + osobne argv + cwd w PTY.
  Nie startuj shella po to, by wkleić `claude<Enter>` lub `codex<Enter>`.
- Shell environment pochodzi z login/interactive shella użytkownika, odczytanego
  w tle z ograniczeniami czasu/rozmiaru. Nie loguj całego środowiska ani sekretów.
- Po zakończeniu zachowujemy ekran i pokazujemy Restart/Close; znamy exit code/sygnał.
  Restart używa aktualnej konfiguracji tego samego profilu i nowego PTY.
- Stop worktree czeka na zakończenie wszystkich jego uruchomionych pane'ów,
  także w nieaktywnych tabach. Zachowuje layout. Nie startuje uśpionych tabów.
- Nie uruchamiaj Restart podczas trwającego stop/cleanup. Ponowny Stop lub
  usuwanie worktree dołącza do istniejącego zadania, zamiast ścigać drugi cleanup.
- Odtworzenie sesji startuje tylko istniejące pane'y aktywnego taba aktywnego
  workspace'u. Inne taby startują przy pierwszym wyborze. Już uruchomione procesy
  mogą działać w tle po zmianie taba/worktree. Nie restartuj ich przy renderze.

## Terminal: zachowaj rozwiązane problemy

- PTY uruchamiaj przez `terminal/spawn.rs`: bezpośredni proces TUI nie może
  dziedziczyć zablokowanego SIGWINCH z wątku dispatch macOS. Maska jest odblokowana
  tylko na czas spawnu i przywracana na tym samym wątku, również po błędzie.
- Tab/Shift+Tab przy fokusie terminala trafiają do PTY (TAB / CSI Z), zamiast
  uruchamiać globalną nawigację Root. Bindings rejestrujemy raz w kontekście
  Terminal; przy fokusie kontrolek Restart/Close zachowujemy nawigację UI.

- Siatka ma stałe komórki. Resztę miejsca rozkładamy symetrycznie; tła skrajnych
  komórek przedłużamy do granic pane'a. Nie dodajemy prawdziwych kolumn ani glifów.
- Podczas resize początek siatki pozostaje zakotwiczony. Po ustaniu zmian następuje
  jednorazowe wycentrowanie, bez animacji przesuwania/gonienia pozycji.
- Pozycje glifów muszą leżeć dokładnie na siatce. Nie polegaj na tolerancji
  force_width, która rozjeżdżała ramki ASCII przy zmianie liczby kolumn.
- Utrzymuj Nerd Font fallback, szerokie/łączone Unicode i wyłączone ligatury
  między komórkami. Nie zastępuj brakujących ikon losowymi symbolami.
- Globalne środowisko procesu zmieniaj tylko na wejściu, przed startem wątków.
  Zmienne profilu przekazuj jako kopię środowiska jego procesu, nie przez globalne set_var.
- Zachowuj xterm-256color/truecolor i izolację flag kolorów launchera. Odziedziczone
  NO_COLOR/CLICOLOR/FORCE_COLOR powodowały brak kolorów Claude/Codex. Nie wymuszaj
  globalnie koloru wbrew jawnej konfiguracji shella użytkownika.
- `clear` ma usuwać także scrollback. Adapter `terminal/clear_scrollback.rs`
  koryguje dokładnie macOS E3 → home → ED2 przez końcowe E3. Zwykłe ED2 oraz
  alternate screen nie mogą kasować historii primary screen przy redraw.
- Nie rozpoznawaj komend przez podsłuchiwanie wpisywanego tekstu. Zachowaj
  obsługę fragmentowanych sekwencji PTY, resize, sygnałów i końcowego outputu.

## SQLite, profile i sekrety

- Aplikacja pracuje na `~/Library/Application Support/Canopy Rust/canopy.db`.
  Bazy Electrona nie używamy jako automatycznego fallbacku i nie nadpisujemy jej.
- Import jest jawną kopią zgodną z obsługiwanym schematem, z read-only źródłem.
  Nie resetuj bazy użytkownika dla ułatwienia testów ani przy błędzie odczytu.
- Snapshot zawiera projekt/worktree, wszystkie workspace'y, kolejność tabów,
  drzew pane'ów, proporcje, fokus, layout i metadane każdego pane'a: cwd, tool ID,
  profile ID, tytuł, argumenty, rodzaj widoku i ewentualne dane wznowienia.
- Nie zapisujemy PID, PTY, uchwytów encji/okien, flag running ani bufora terminala.
- Jeden writer scala zmiany layoutu; generacje chronią przed utratą zmian w locie.
  Identyczny snapshot nie wywołuje zapisu. Nowsze/uszkodzone schematy odrzucamy,
  zamiast automatycznie nadpisywać. Błędy mają pozostawić dane w pamięci.
- Quit czeka na operacje, końcowy zapis i cleanup. Błąd końcowego zapisu pozostawia
  aplikację otwartą. Nie obiecuj trwałości niezapisanych zmian przy force kill.
- ToolDefinition i Profile to oddzielne modele. Własne toole i wiele profili
  mają stabilne ID; zmiana domyślnego profilu nie przepina istniejących pane'ów.
- Usunięty profil/wyłączone narzędzie daje jawny błąd; nie uruchamiaj potajemnie
  innego profilu lub shella. Zapis konfiguracji nie restartuje żywych procesów.
- Preferencje agentów odwzorowują formularze Electrona: lista profili po lewej,
  sekcje model/permissions/provider/API/env/JSON. Różnice aktualnego CLI obsługuj
  w adapterze; przykład: Full auto Codexa jako sandbox workspace-write + on-request.
- API key jest maskowane i przechowywane w macOS Keychain. SQLite ma tylko
  odwołanie. Puste pole zachowuje klucz; usunięcie jest osobną akcją. Nie loguj
  kluczy i nie kopiuj automatycznie sekretów Electron safeStorage.
- Custom env jest jawną konfiguracją SQLite mimo maskowania w UI. Nie udawaj,
  że maskowanie oznacza szyfrowanie. Nadpisania JSON/profili nie mogą zmieniać
  globalnej konfiguracji/logowania użytkownika ani kolidować między procesami.

## Git i bezpieczne usuwanie worktree

- Cały Git poza wątkiem UI. Jeden ograniczony worker, cache metadanych, scalane
  notyfikacje. Workspace'y jednego repo współdzielą snapshot metadanych.
- Obserwuj metadane, nie pełne pliki robocze: HEAD, refs, worktrees, packed-refs,
  config. Ignoruj zdarzenia objects i zwykłych plików dla tego odczytu.
- Bez cyklicznego status/diff i pollingu worktree w spoczynku. Serię zdarzeń scala
  debounce; callback filesystemu tylko sygnalizuje dirty. Nie odkładaj odczytu
  w nieskończoność przy ciągłym napływie zdarzeń. Zwolnij watchery po zamknięciu.
- Pełny status wykonuj na żądanie przed usunięciem oraz dla widocznych Changes/diffów,
  nie przy renderze. Watcher plików aktywnego worktree scala zdarzenia i filtruje ignored.
  Awaria watchera ma być widoczna; pozostaw możliwość ręcznego Refresh.
- Nowe worktree powstaje w nowym katalogu poza istniejącymi working trees.
  Canopy zarządza nazwą `<repo>-<10 znaków UUID>` i ponawia propozycję przy
  kolizji. Odrzucaj branch zajęty w innym worktree i nie nadpisuj istniejącego
  katalogu ani symlinka.
- Usuwanie ma sekwencję modali: zgoda na zamknięcie procesów → oczekiwanie na
  PTY → ponowny status → osobna zgoda na trwałe usunięcie local/untracked/ignored.
  Użytkownik może wymusić usunięcie tych zmian dopiero tym potwierdzeniem.
- Anulowanie drugiego kroku zachowuje pliki i layout; procesy wcześniej
  zamknięte za zgodą nie restartują się automatycznie.
- Główne worktree, lock, detached HEAD, submoduły i niedokończona operacja Git
  nadal mają osobne zabezpieczenia. Domyślne usuwanie zachowuje branch; jawnie
  wybrane usunięcie lokalnego brancha następuje dopiero po cleanupie i kontroli
  ref/OID, zajętości oraz commitów nieosiągalnych z wybranego celu.
- Merge przed usunięciem dotyczy wyłącznie lokalnych commitów. Konflikt lub błąd
  zachowuje worktree; kolejność to merge → potwierdzony cleanup → opcjonalny branch.
  Cel checkoutowany musi mieć czysty indeks/working tree i zatrzymane procesy Canopy.
- Brakujący katalog ma osobną akcję usunięcia nieaktualnej rejestracji Git
  i zapisanego workspace'u. Wymaga potwierdzenia; nie kasuje working tree ani
  branchy. Ponownie sprawdzaj nazwę, ścieżkę, brak katalogu i locki. Nie traktuj
  błędów dostępu, uszkodzonego istniejącego katalogu ani symlinków jako braku.
  Zachowuj tożsamość ścieżki przez kanonizację istniejącego rodzica.
- Potwierdzenia są typowanym wynikiem operacji, nie parsowaniem tekstu błędu.
  Nie pozwól, aby szybki double-click potwierdził dwa kolejne destrukcyjne kroki.
- Procesy poza Canopy nie są automatycznie wykrywane. Nie deklaruj inaczej.

### Pull, Push i upstream

- Pull/Push wykonuje libgit2 na istniejącym workerze, tylko po akcji użytkownika.
- Pull to fetch + fast-forward; divergence nie uruchamia automatycznie merge/rebase.
  Chronimy staged/unstaged/untracked oraz ignorowane pliki przed nadpisaniem.
- Push ma jawny refspec bieżącego brancha/OID bez force. Sprawdzamy także
  push_update_reference: transport OK nie oznacza akceptacji przez serwer.
- Brak upstreamu otwiera modal remote/nazwa gałęzi (domyślnie lokalna nazwa).
  Zapis upstreamu dopiero po udanej operacji. Błąd zapisu po pushu nie może
  być przedstawiony jako brak wysłania commita.
- SSH próbuje ssh-agent, potem domyślne pliki kluczy (każdy tylko raz);
  klucze szyfrowane wymagają odblokowania w agencie. HTTPS używa macOS Keychain. Nie akceptujemy niezweryfikowanych
  certyfikatów/host keys i nie wykonujemy git credential helper w shellu.
- Globalne timeouty libgit2 ustawiamy wyłącznie w main przed startem wątków.
- Zachowuj blokadę operacji, anulowanie i oczekiwanie przy quit. Anulowanie
  po akceptacji pushu nie cofa zdalnej zmiany. Szczegóły: docs/git-network.md.

### Commit i podpisy

- Commit obejmuje staged index, nigdy nie stage'uje plików potajemnie.
- Honoruj commit.gpgSign i format podpisu. GPG/ssh-keygen lub skonfigurowany
  signer mogą być subprocessami; operacje Git pozostają w libgit2.
- Hasło obsługuje gpg-agent/pinentry albo ssh-agent/SSH_ASKPASS. Nie buduj
  formularza hasła w Canopy i nie dodawaj unsigned fallbacku po błędzie/anulowaniu.
- Po podpisie sprawdź ponownie HEAD/indeks przed publikacją. Nie trzymaj blokad
  repozytorium przez czas pytania o hasło. Draft wiadomości zostaje po błędzie.
- Zapisuj kontekst diffu (cwd, ścieżka, staged/unstaged) w metadanych pane'a.
  Widoczny diff i lista zmian są wirtualizowane i odświeżane poza renderem.
- Commit uruchamia pre-commit → prepare-commit-msg → commit-msg przed podpisem,
  a post-commit po publikacji. Hooki mogą zmieniać indeks i wiadomość; przed
  publikacją ponownie sprawdzamy HEAD/indeks. Brak globalnego bypassu hooków.
- Hooki są programami użytkownika i mogą wywoływać własne narzędzia (również git);
  operacje Git implementowane przez Canopy nadal korzystają z libgit2.
- Błąd post-commit oznacza ostrzeżenie przy utworzonym commicie, nigdy rollback
  ani komunikat o braku commita. Szczegóły: docs/git-hooks.md.

## Sesje agentów i resume

- Claude/Codex używają wspólnego odbiornika hooków i oddzielnych adapterów.
- Resume ID dostawcy zapisujemy w metadanych pane'a po otrzymaniu zdarzenia.
  Run ID i token są nowe przy każdym starcie i nie trafiają do SQLite.
- Wznowienie wskazuje konkretny UUID. Nie stosuj resume latest ani cichego
  tworzenia nowej sesji po błędzie. Restart ponawia, New session jest jawne.
- Konfigurację hooków dokładamy do prywatnej kopii profilu przez argv.
  Nie zapisujemy .codex/hooks.json w projekcie i nie nadpisujemy hooków usera.
- Respektuj zaufanie hooków Codexa; nie dodawaj globalnego bypassu.
- Inspektor dotyczy wybranego pane'a. Notch agreguje działające sesje i kieruje
  do konkretnego pane'a. request_user_input / AskUserQuestion dają Needs attention.
- Zakończenie innego toola/subagenta nie może usuwać aktywnego oczekiwania.
- Notch automatycznie sygnalizuje istotne zmiany niewidocznych agentów.
  Widoczność: aktywne główne okno + wybrany workspace/tab; każdy pane splitu.
  Pokazanie pane'a potwierdza obejrzenie, zwykłe Working i jawne Stop nie alarmują.
- Live Claude pozostaje niezweryfikowane do czasu logowania; patrz docs/agents.md.

## Toasty

- Lekkie sukcesy operacji (Pull/Push, commit) trafiają do AppState.toasts.
- Jeden aktywny toast, dwie krawędzie pod spodem, ograniczona kolejka FIFO.
  Każdy dostaje 4 s po aktywacji; pauza na hover, X, POPOVER i Reduce Motion.
- Toast nie przejmuje focusu, nie zmienia layoutu i nie wywołuje powiadomień OS.
- Błędy wymagające uwagi pozostają inline; decyzje wymagają modalu.
- Nie dodawaj toastów dla każdej czynności, autosave czy odświeżenia. docs/toasts.md.

## Notch macOS

- To osobne, nieaktywujące okno/panel na pasku menu, nie poniżej jego usable area.
- Ramka natywnego okna pozostaje stała; animujemy widoczną wyspę. Dynamiczne
  resize NSPanel powodowało znikanie notcha/czarne prostokąty i zostało odrzucone.
- Przezroczysty obszar poza widoczną wyspą przepuszcza kliknięcia. Nie może
  blokować tabów przeglądarki ani zabierać focusu po zwinięciu.
- Lokalne/globalne monitory myszy oceniają rzeczywistą pozycję względem
  animowanej geometrii, także rogów. Sam layoutowy on_hover powodował ponowne
  otwieranie podczas zamykania. Monitory mają właściciela i cleanup.
- Z callbacków AppKit przechodź przez executor GPUI; unikaj reentrant update.
  Testuj prawdziwą myszką — syntetyczne zdarzenie nie dowodzi poprawnego hoveru.

## Weryfikacja, wydajność i praca z repo

- Przed zmianą sprawdź Git status i zachowaj niezwiązane edycje użytkownika.
  Na `commit` zapisuj uzgodniony zakres; podaj hash i stan repo. Bez push, jeśli
  użytkownik go nie zlecił. Nie commituj automatycznie po każdej poprawce.
- Lint/format: `cargo fmt --all -- --check` oraz
  `cargo clippy --locked --all-targets -- -D warnings`.
  Build po autoryzacji zakresu: `cargo build --locked`.
  Dobieraj dozwolone sprawdzenia do zmiany; nie powtarzaj pomyślnych sprawdzeń
  bez nowej zmiany, błędu lub nierozstrzygniętej wątpliwości.
- Dobór wartościowych testów i granice weryfikacji: [testing.md](testing.md).
- Release aplikacji: `./scripts/build-macos.sh release`, wynik
  `target/release/Canopy.app`. Profilowanie: profil `profiling`, opcjonalnie
  feature `frame-profile`; dev-inspector sprawdzaj, gdy zmiana go dotyczy.
- Płynność oceniaj w release/profiling, nie samym debug. Biblioteka nie gwarantuje
  120 FPS. Rozdziel czas CPU draw/layout, GPU i fizyczną prezentację klatek.
- Używaj osobnych encji i cache z określonymi granicami, gdy niezmieniony sibling
  jest przeliczany podczas scrollu. Najpierw profiluj; nie dodawaj pętli redraw.
- Zmiany GUI sprawdzaj w działającej aplikacji: hover in/out, szybkie toggle,
  resize, overscroll, focus, drag/drop, puste stany i restore według zakresu.
  Sam build/test modelu nie oznacza weryfikacji GUI lub animacji.
- Przy Computer Use stosuj właściwy skill. Gdy użytkownik zmieni aplikację,
  pobierz świeży stan przed kolejną akcją. Nie nazywaj przerwanej próby sukcesem.
- Rozróżniaj zbudowany release od uruchomionego: działający proces może nadal
  używać starego binarium. Restart dobieraj do kontekstu i aktywnych procesów;
  nie przerywaj niepowiązanej pracy ani nie kasuj layoutów/buforów dla testu.
- Dla izolowanego GUI można jawnie ustawić CANOPY_DATA_DIR; nie zmieniaj HOME
  ani domyślnej bazy użytkownika dla testów.
- Testy Git/usuwania/sekretów wykonuj na kontrolowanych repo i wpisach testowych.
  Nie usuwaj prawdziwego worktree ani kluczy użytkownika do demonstracji funkcji.
- Raportuj konkretnie: co zmieniono, jakie sprawdzenia przeszły, co jest
  niezweryfikowane. Nie traktuj dawnych liczb testów/pomiarów jako aktualnych.

## Aktualne granice i dalsze źródła

Terminal/PTY, projekty i worktree, pełny layout, persist/restore, narzędzia oraz
profile mają działające integracje. Files ma backend rzeczywistych plików, a edytor obsługuje zapis, konflikty i restore
([zakres i ograniczenia](files-editor.md)). Git Changes/diffy i commit mają integrację
([szczegóły i ograniczenia](git-changes.md)). Inspektor, notch i resume mają
integrację sesji Claude/Codex ([zakres kwalifikacji](agents.md)); część
Preferences i metryki zasobów nadal mają mocki. Pełny edytor pozostaje poza zakresem. Nie rozszerzaj tych deklaracji bez sprawdzenia kodu.

- [Komponenty](components.md), [motion](motion.md)
- [Stan aplikacji](app-state.md), [persist/restore](persistence.md)
- [SQLite](settings.md), [narzędzia](tools.md), [profile agentów](agent-preferences.md)
- [Terminal](terminal.md), [Git/worktree](git-worktrees.md), [Git Changes i podpisy](git-changes.md)
- [Notch](notch.md), [profilowanie](performance.md), [weryfikacja](verification.md)

Starsze fragmenty docs/ opisują kolejne etapy migracji. Jeśli opis mocka przeczy
aktualnemu kodowi i powyższym kontraktom, zweryfikuj moduł; nie cofaj działającej
funkcjonalności do dawnego etapu tylko po to, by dopasować ją do starego tekstu.

## Pliki i edytor

- Files i Cmd+P dotyczą aktywnego worktree; indeksowanie i parsowanie plików
  wykonuj poza UI. Zachowuj ostrzeżenia przy limitach i błędach odczytu.
- Zamknięcie taba, pane'a, projektu i aplikacji chroni dirty buffers przez
  Save / Discard / Cancel. Nie dodawaj nowych ścieżek zamknięcia omijających Editors.
- Zmiana pliku przez agenta przeładowuje czysty dokument; dirty buffer zostaje
  w pamięci z konfliktem. Zapis nie może bez pytania nadpisać nowej wersji na dysku.
- Zachowuj atomowy zapis, UTF-8 BOM, CRLF i uprawnienia. Metadata edytora
  przeżywa restore, ale buffer/undo nie jest zapisany w SQLite.
- Szczegóły: [Files i edytor](files-editor.md).
- Files nie ma własnego scrolla ani limitu wysokości sekcji. Jedyny pionowy
  viewport należy do całego sidebara; wirtualizacja FileTree oraz odsłanianie
  zaznaczenia klawiaturą korzystają z jego ScrollHandle.

- Pojedynczy klik pliku w Files otwiera go w edytorze (lub wybiera istniejący pane).
  Kliknięcie folderu rozwija/zwija go; Enter zachowuje tę samą semantykę.
- Pliki graficzne (PNG/JPEG/WebP/GIF/BMP/ICO/TIFF/SVG) otwieramy jako read-only
  ImagePreview z dopasowaniem proporcji. Nie kieruj ich do edytora UTF-8.
  Image pane zapisuje cwd/resource/kind, bez bufora obrazu ani procesów w SQLite.
- Fonty TTF/OTF/WOFF/WOFF2 mają read-only specimen bez instalacji i globalnego
  rejestrowania fontu. Nie zastępuj brakujących glifów fontem systemowym.
- Podgląd wideo korzysta wyłącznie z systemowego AVFoundation: MP4/MOV/M4V,
  a MKV tylko gdy obsłuży go system. Bez FFmpeg i transkodowania. Ukrycie pane'a,
  modal oraz zamknięcie zatrzymują player i chowają natywną warstwę.

## Integracje

- Start aplikacji musi instalować rzeczywisty klient HTTP przez with_http_client;
  domyślny natywny klient GPUI nie wykonuje połączeń. Transport ma testy prawdziwego
  HTTP; sam mock adaptera nie weryfikuje połączenia aplikacji z dostawcą.
- Tasks i CI/CD to odrębne moduły korzystające ze wspólnej konfiguracji kont.
  Obecne adaptery: GitHub.com Issues, Jira Cloud i YouTrack; Actions/TeamCity pozostają kolejnymi etapami. Szczegóły: [Jira](jira.md), [YouTrack](youtrack.md).
- Lista GitHub Tasks używa GraphQL repository.issues, po 30 rzeczywistych issues, z kursorem
  i totalCount. Nie wracaj do paginacji mieszanych issues/PR, która dawała niepełne
  strony po lokalnym filtrowaniu. Błąd GraphQL nie publikuje częściowej strony ani kursora.
- Repozytorium GitHub Tasks wykrywamy z origin; jawne nadpisanie wspólne dla worktree
  jednego repo ma pierwszeństwo i nie może zniknąć po odświeżeniu remota.
- Tokeny integracji trafiają wyłącznie do osobnej usługi Keychain. SQLite ma
  referencje, konfigurację i linki zadań. Nie importuj automatycznie sekretów gh/Electrona.
- Wspieramy domyślny classic PAT oraz osobne tokeny per organizacja/właściciel.
  Właściciel repo wybiera połączenie; dopasowanie organizacji ma pierwszeństwo
  przed domyślnym. Nie próbuj innych tokenów po błędzie dostępu. Wymiana/usunięcie
  dotyczy jednego połączenia; nie odłączaj pozostałych organizacji.
- Link tworzenia PAT wypełnia uprawnienia bieżącej funkcji (Issues), bez zapasu
  uprawnień pod przyszłe moduły. Classic repo daje szerszy dostęp niż odczyt.
- Nie przenoś cache zadań między kontami. Anuluj spóźnione wyniki po zmianie
  workspace/repo/konta. API ma limity czasu/rozmiaru, bez pollingu w spoczynku.
- Powiększone szczegóły taska korzystają ze wspólnego modala i motion POPOVER;
  zachowuj Escape, zwrot focusu, blokadę skrótów workspace, ukrycie natywnego wideo
  oraz informację dla notcha, że agent jest zasłonięty. Kliknięcie agenta zamyka podgląd.
- Komentarze pobieramy na żądanie przez TaskProvider, z paginacją i anulowaniem przy
  zamknięciu. Bez pollingu. Zapis issues/komentarzy jest jawną akcją użytkownika;
  jedna operacja zapisu należy do IntegrationsState i jest oczekiwana przy quit.
- Drafty issues/komentarzy mają osobną tabelę SQLite i klucz połączenie/login/repo/rodzaj.
  Nie usuwaj tekstu po błędzie. Normalny quit czeka na zapis draftów; błąd zostawia aplikację otwartą.
- Nie ponawiaj automatycznie POST po timeoutach lub niejednoznacznym wyniku. Ostrzeżenie
  o możliwym zapisie przeżywa restore wraz z draftem. HTTP 201/204 oznacza zapis;
  błąd późniejszego odświeżenia nie może udawać, że operacja nie została wykonana.
- Fine-grained PAT do edycji potrzebuje Issues: write oraz Metadata: read; istniejący
  classic repo pozostaje obsługiwany. Brak uprawnień jest błędem operacji, bez fallbacku na inne konto.
- Labels/assignees dodawaj i usuwaj pojedynczo, nie zastępuj ich całej listy z nieaktualnego
  snapshotu. GitHub może zignorować metadata przy zbyt małych prawach — pokazuj częściowy wynik.
- Usunięcie komentarza wymaga potwierdzenia przy tym komentarzu. Nie twórz próbnych wpisów
  w rzeczywistych repozytoriach podczas testowania funkcji zapisu.
- Szczegóły: [integrations.md](integrations.md).

### Jira Cloud

- Jira jest drugim TaskProvider; routing uwzględnia site + project key, a tożsamość
  użytkownika to accountId. Nie utożsamiaj jej z emailem ani displayName.
- Email/site/Cloud ID należą do konfiguracji; token wyłącznie do Keychain.
  Scoped token używa api.atlassian.com i weryfikuje baseUrl przed zapisaniem połączenia.
- Projekty przypisujemy jawnie w Tasks; Git origin nie mapuje automatycznie Jira.
- Create/edit/transition formularze wynikają z metadata Jira, z required fields
  i allowedValues. Nie koduj na sztywno statusów ani ID pól sprintu.
- Przechowuj oryginalne ADF. Nietłumaczalne elementy zachowują lokalne referencje
  podczas edycji i są odtwarzane przed zapisem. Nie spłaszczaj opisów do plain text.
- Zmiana pola wysyła tylko to pole. Nie retry'uj niepewnych zapisów. Reload po
  zaakceptowanej mutacji może dać warning, ale nie zmienia sukcesu w porażkę.
- Usunięcie issue wymaga wpisania klucza i osobnej opcji deleteSubtasks;
  nie persistuj treści takiego potwierdzenia. Usunięcia attachments/links sprawdzają
  przynależność do taska ponownie przed wykonaniem.
- Jira search używa nextPageToken oraz reconcileIssues dla ostatnich mutacji.
  GUI i realne tenant API kwalifikujemy oddzielnie od testów mock HTTP.

### Task browser i załączniki

- Dropdown szybkiego przełączania Jira wybiera projekty (np. GAKKO / ISSUE), nie
  tablice Agile. Zmiana zapisuje istniejące mapowanie repo bez dodatkowego Apply.
- Filtry Jira zastępują segment Active/Done: nie dokładaj starego statusu do JQL.
  Wbudowane currentUser()/openSprints() są dynamiczne. Własne filtry konfigurujemy
  lokalnie w Preferences → Task filters; nie publikujemy ich automatycznie do Jira.
- JQL zawsze pozostaje przecięty z wybranym projektem. Cache i odrzucanie wyników
  uwzględniają site/projekt/konto/wyszukiwanie/wyrażenie. Wybór filtra przeżywa restart.
- Załącznik otwiera osobne okno Canopy z natywnym Quick Look; download jest osobną
  akcją. Prywatne tempdir 0700 / plik 0400, limit 25 MiB, bez zapisywania ścieżki
  tymczasowej jako trwałego pane'a. Native owner utrzymuje plik do końca podglądu.
- Nie przekazuj tokenów ani remote URL do Quick Look. Esc/Cmd+W monitora tylko
  wysyła zdarzenie na executor GPUI; close zwalnia monitor i QLPreviewView.
- Szczegóły i granice weryfikacji: [task-browser.md](task-browser.md).

### Files: odczyt na żądanie

- Start projektu czyta tylko bezpośrednie dzieci root. Rozwinięcie katalogu czyta
  jeden następny poziom; zwinięcie usuwa jego dane z cache drzewa. Nie przywracaj
  globalnego rekurencyjnego indeksowania 50 tys. wpisów przy starcie/odświeżeniu.
- Ignorowane przez Git pliki/katalogi są widoczne, ale przyciemnione, także .env.
  Sama kropka nie oznacza ignorowania. Śledzony wpis nie jest ignored tylko przez
  pasującą regułę. Nie ukrywaj poprawnego selected/hover background.
- Osobne zdarzenia FileTree odpowiadają za otwarcie pliku i zmianę rozwiniętych
  katalogów. Loading/error i retry katalogu nie mogą uruchomić edytora.
- Git status jest osobnym snapshotem; untracked directories bez rekurencyjnego
  wyliczania plików. Obserwacja uwzględnia rodziców śledzonych plików z Git index,
  aby dekoracje zamkniętych folderów nadal się aktualizowały.
- Na macOS FSEvents nasłuchuje root bez wyliczania potomków; filtr callbacka pomija
  nieotwarte zależności/objects. Nie rejestruj osobnego streamu na każdy katalog.
- Cmd+P wyszukuje na żądanie poza cache drzewa, z ignore rules, debounce i anulowaniem.
  Nie ograniczaj wyszukiwarki po cichu do katalogów już rozwiniętych w Files.
- Retarget doładowanych wierszy zachowuje sąsiadów i bieżący progress. Sama zmiana
  statusu Git/ignored/loading nie restartuje animacji.

### Etykiety worktree

- Detached HEAD nie jest branchem o nazwie `HEAD`. Model rozróżnia branch/unborn,
  detached commit i niedostępny HEAD. Operacje korzystają z branch_name(), nie label.
- Sidebar pokazuje branch, a przy detached/unavailable nazwę katalogu, z fragmentem
  rodzica przy niejednoznaczności (np. pinpoint (bb23)). Tooltip zawiera path i commit.
- Path i WorkspaceId nadal identyfikują workspace; label nie wpływa na resume ani layout.

### Czytelność Session

- Session ma jeden pionowy scroll pod tabami inspektora. Wewnętrzny stos i sekcje
  nie kurczą się do wysokości viewportu; metadane, odpowiedź i aktywność nie nachodzą
  na siebie ani na status bar.
- Pytanie i ostatnia odpowiedź używają wspólnego MarkdownView z kompaktowymi tokenami.
  Zmiana statusu nie resetuje scrolla; zmiana pane/run resetuje go i czyści stary tekst.
- Nazwy narzędzi, zdarzeń i trybów są prezentacyjne, a profile używają nazwy katalogu
  profili. Surowe ID pozostają w tooltipach i logice integracji; nie zmieniaj matchowania
  hooków/resume na podstawie czytelnych labeli.
