# Komponenty interfejsu Canopy

Punkt wejścia: `crate::ui::components`. Komponenty korzystają z tokenów
`ui::theme` i z GPUI Kit 0.7.1. Istniejące Preferences i sidebar już ich używają.

## Dostępne elementy

| API | Przeznaczenie | Właściciel stanu |
| --- | --- | --- |
| `selection_button(id, label, selected)` | Wspólny przycisk wyboru narzędzia/profilu | wybór i callback w widoku |
| `form_field(label, help, control)` | Etykieta, dowolna kontrolka i opcjonalny opis; pusty help nie zajmuje miejsca | prezentacja, stan kontrolki u wywołującego |
| `field_grid(fields)` | Dwie równe kolumny z etykietami wyrównanymi do góry; wiersze zachowują naturalną wysokość w scrollu | prezentacja |
| `Disclosure::measured_body(id, content, now)` | Rozwijanie treści o zmiennej wysokości, mierzonej przez GPUI Kit; wspólne motion, bez dodatkowego scrolla | stan Disclosure w widoku |
| `button(id, label)` | Przycisk tekstowy w stylu Canopy | callback / widok nadrzędny |
| `icon_button(id, icon, accessible_label)` | Kompaktowy przycisk z tooltipem i nazwą dostępności | callback / widok nadrzędny |
| `untitled_icon_button(id, icon, accessible_label)` | `icon_button` bez tooltipa, gdy kontrolkę objaśnia karta na hover wokół niej | callback / widok nadrzędny |
| `input(&state)` | Wspólna baza pola tekstowego; wariant osadzony przez `.appearance(false)` | `Entity<InputState>` w widoku |
| `dropdown(&state)` | Select z dowolnym delegatem GPUI, wspólny trigger i menu | `Entity<SelectState<D>>` w widoku |
| `checkbox(id, checked, label)` | Checkbox z nazwą dostępności | bool w widoku |
| `labeled_checkbox(id, checked, label)` + `checkbox_help(text)` | Checkbox z widoczną, klikalną etykietą (ta sama nazwa dostępności); pomoc jako dziecko pod etykietą, 11 px muted jak w `form_field` | bool w widoku |
| `modal::Confirmation` (+ `.destructive()`), `TextPrompt` (+ `.confirm_label(…)`) | Potwierdzenie z czasownikiem wyniku i opcjonalnym stylem destrukcyjnym; prompt jednego pola z nazwaną akcją | encja modala i `ModalDismissed` u właściciela |
| `FileTree` + `FileTreeModel` + `FileNode` | Hierarchia plików/folderów, selection, expand i zdarzenie otwarcia | osobna `Entity<FileTree>` |
| `section`, `setting`, `caption` | Sekcje ustawień, etykiety, pomoc i separatory | prezentacja |
| `row`, `column`, `icon`, `custom_icon`, `tool_icon`, `badge`, `dot` | Układ i drobne elementy prezentacyjne | prezentacja |

Moduły: `buttons.rs`, `controls.rs`, `settings.rs`, `file_tree.rs` oraz wspólne
helpery w `components/mod.rs`. Dane referencyjnego drzewa są w `sidebar.rs`,
a nie w komponencie. Komponent nie otwiera ani nie odczytuje plików.

## Stylowanie i rozmiary

To GPUI, więc odpowiednikiem `class` i jego scalania jest builder `Styled`.
Helpery zwracają natywne typy GPUI/GPUI Kit, a nie zamknięte `impl IntoElement`:
wywołujący może nadpisać szerokość, padding, kolor, margines lub callback.
Nie dodajemy sztucznego parsera klas CSS. Marginesy między komponentami
należą do miejsca użycia; wnętrze kontrolki i odstępy etykieta/pomoc są jej stylem.

```rust
use crate::ui::components::{button, checkbox, dropdown, input};
use gpui_kit::*;

// W render: encje pól/selectów zostały utworzone wcześniej, w new().
let name = input(&self.name).w(px(260.));
let tool = dropdown(&self.tool).w(px(160.));
let save = button("save-settings", "Save")
    .on_click(cx.listener(|this, _, _, cx| {
        this.save_requested = true;
        cx.notify();
    }));
let enabled = checkbox("enabled", self.enabled, "Enable integration")
    .on_click(cx.listener(|this, value, _, cx| {
        this.enabled = *value;
        cx.notify();
    }));
```

ID powinno być stabilne i unikalne w danym kontenerze. Etykieta może się zmieniać
niezależnie od ID. Natywny builder zachowuje obsługę disabled/loading, focusu,
klawiatury i menu; używaj traitów GPUI Kit, np. `Disableable`, kiedy potrzebne.
Tekstowe `button` ma wariant neutralny Canopy. Inputy nie implementują własnej
edycji/IME/schowka; konfiguruj natywny `InputState`, nie osobną kontrolkę dla
każdego typu pola.

## Drzewo plików

```rust
use crate::ui::components::file_tree::{FileNode, FileTree, FileTreeModel, FileTreeEvent};

let model = FileTreeModel::new(
    vec![FileNode::folder("src", "src", vec![
        FileNode::file("src/main.rs", "main.rs"),
    ])],
    ["src".into()],
).expect("unique IDs");
let tree = cx.new(|cx| FileTree::new(model, cx));
// Zachowaj `tree` i Subscription jako pola widoku, nie twórz ich w render.
let subscription = cx.subscribe(&tree, |this, _, event: &FileTreeEvent, cx| {
    this.requested_file = Some(event.id.clone());
    cx.notify();
});
// W render: skończony viewport jest obowiązkowy.
let viewport = div().h(px(364.)).w_full().child(self.tree.clone());
```

- ID może być ścieżką względną lub trwałym identyfikatorem domenowym. Równe
  etykiety są dozwolone, duplikaty ID są odrzucane przez konstruktor.
- `children: None` oznacza plik; `Some(vec![])` pusty folder.
- Początkowe rozwinięcia podaje caller. Zwijanie zachowuje rozwinięcia potomków.
- Widoczne wiersze są przygotowywane przy zmianie rozwinięć; `render` nie
  przechodzi po całej hierarchii. Układ wirtualny wyszukuje zakres viewportu binarnie i tworzy tylko widoczne wiersze.
- Kliknięcie folderu rozwija/zwija; strzałki obsługują selection i hierarchię
  po sfokusowaniu drzewa kliknięciem. Enter/pojedynczy klik pliku emituje `FileTreeEvent`.
- W mocku konsument pokazuje ścieżkę żądanego pliku w status barze. Nie otwiera
  prawdziwego edytora ani nie wykonuje I/O.
- W sidebarze wysokość viewportu dopasowuje się do liczby widocznych pozycji,
  do maksymalnie 13 wierszy. W innym widoku caller może podać własną wysokość.

Ta pierwsza wersja nie implementuje filesystem watchera, ładowania dzieci,
zmiany danych w miejscu, drag/drop, menu kontekstowego ani multiselection.
Są to przyszłe rozszerzenia modelu, a nie ukryte obowiązki komponentu renderującego.

## Weryfikacja

Testy modelu: duplikaty ID, identyczne etykiety w różnych gałęziach, collapse
z ukrytą selection, ponowne rozwinięcie i rozróżnienie pustego folderu od pliku.
Sprawdzone formatowanie, Clippy, testy i release. GUI: rozwijanie/zwijanie,
strzałki oraz Enter i odbiór zdarzenia w status barze; przegląd Preferences.
Cache encji treści/nawigacji Preferences pozostaje zachowany.

Krótka próba po refaktorze: 30 s przewijania nawigacji Preferences w
`profiling + frame-profile`, 276 poleceń przewijania, 285 draw.
p50 2.776 ms, p95 3.110 ms, p99 3.642 ms, max 8.013 ms,
0% ponad 8.333 ms. CSV: `target/performance/components/frames-1788880655342.csv`.
To kontrola regresji kosztu draw; nie pomiar fizycznych 120 FPS ani pełna
kwalifikacja (jedna próba, poniżej 1200 próbek).

W GUI potwierdzono także wpisywanie polskich znaków w input oraz otwarcie
menu dropdownu po ekstrakcji.

## Notch (macOS)

`components/notch.rs` udostępnia elementy prezentacyjne:

- `notch_surface(id, size, radius)` — czarna, przycinana powierzchnia.
- `notch_header(height, status_color)` — belka ikon z elastyczną przerwą.
- `notch_session_row(id, &NotchSession)` — wiersz sesji ze statusem.
- `NotchSession` — workspace, context, status i status_color.

```rust
let session = NotchSession {
    workspace: "my-project".into(),
    context: "main · agent session".into(),
    status: "Idle".into(),
    status_color: theme::notch_idle(),
};
let item = notch_session_row("session-42", &session)
    .on_click(|_, _, cx| { /* obsługa wyboru przez aplikację */ });
```

Komponenty zwracają `Div` / `Stateful<Div>`, więc zachowują builder Styled.
Nie posiadają timera, referencji do okna ani własnych danych mocka. Padding
kontenera treści i ruch należą do `ui/notch.rs`; ten kontroler nadal odpowiada
za wejście/wyjście myszy. `notch_motion.rs`, `notch_macos.rs` i
`notch_windows.rs` zachowują osobne odpowiedzialności. Refaktor nie zmienia
tokenów ani geometrii produktu.


## Zwijane sekcje i drzewo

`Disclosure` przechowuje stan rozwinięcia oraz dwa wspólne `Presence`:
PANEL dla wysokości i CONTENT_REVEAL dla treści. `header` zwraca przycisk,
`body` przycina treść do bieżącej wysokości i stosuje opacity/przesunięcie.
Sidebar pokazuje projekty, ich worktree i zagnieżdżone rozmowy oraz osobne
Private chats. Mierzone body zachowuje wysokość przy dowolnej liczbie rozmów.
Sekcje, projekty i worktree zwijają się tym samym `Disclosure` (wysokość, fade
treści i obrót strzałki w obie strony). Worktree dostaje stan dopiero przy pierwszym
przełączeniu (`Sidebar::worktree_groups`); brak wpisu oznacza otwarty.
Wiersze rozmów projektowych i prywatnych mają ten sam układ (kafelek agenta, tytuł,
status · model); kontekst jest na rodzicu oraz w karcie sesji na hover. Agenci w PTY mają glif
terminala; ich wybór aktywuje istniejący tab terminala. Rodzice agregują stan pracy. Files i Tasks nie mają kopii po lewej:
Files/Changes/Tasks/Agent renderuje dotychczasowy Inspector we wspólnym panelu
narzędzi; terminale i CLI są tabami w centrum, obok rozmów. Wąski panel ma bieżącą
nazwę strony i jej alternatywy w menu. Szczegóły interakcji i własności: [układ workspace'u](features/workspace-layout.md).

Nagłówki mają przezroczyste tło także na hover/press. Jawny stan `hovered`
zmienia kolor tekstu/strzałki z faint na secondary; nie polegamy na nadpisywanym
przez GPUI stylu hover wariantu Ghost. Focus klawiatury pozostaje natywny.

FileTree używa teraz animowanego układu w `file_tree/motion.rs` zamiast
uniform_list: po toggle przygotowuje uporządkowaną sumę starych i nowych
wierszy, interpoluje pozycje, wysokość obszaru i przezroczystość. Wiersze
wychodzące pozostają do końca fade, ale nie obsługują kliknięć. Następnie
są usuwane. Przebudowa indeksu odbywa się przy zmianie rozwinięć, a render
wyszukuje zakres widoczny i pomija niewidoczne wiersze. Kąt strzałki folderu
również jest interpolowany. Szybkie odwrócenie zaczyna się od bieżącej geometrii.

Sekcje i drzewo korzystają z motion::policy (w tym Reduce Motion), bez timerów
idle. Wysokości sekcji nie są ściskane przez flex; overflow sidebara przewija się.
Dodane testy: retencja/usunięcie wychodzących pozycji, odwrócenie bez skoku
oraz wirtualny zakres przy 10 000 wierszy. Łącznie 19 testów przechodzi;
sprawdzone fmt, Clippy i build release. Użytkownik potwierdził poprawny hover.

## Sidebar: tożsamość i aktywność agentów

Kod: `sidebar/activity.rs` (stan, kafelki, obrys, znaczki, podsumowania), wiersze
w `sidebar/chats.rs`, projekty i worktree w `sidebar/projects.rs`, tokeny w `theme.rs`.

- `Activity` wynika ze wspólnego `navigation_status` i `AgentSession::unseen`.
  Etykiety: Working, Goal, Needs you, Done, Idle, Error, Starting…, No status,
  Loading…, Stopping…, Exited, Not started; karta sesji dopisuje wyjaśnienie.
- Kafelek 1.5rem w kolorze znaku providera (`agent_tile`, `agent_tile_edge`).
  Exited jest neutralny, Not started ma sam kontur.
- Obrys aktywności rysuje `canvas` + `PathBuilder` po zaokrąglonym kwadracie
  koncentrycznym z kafelkiem (nie okrąg): Working/Goal — jasny odcinek 24% obwodu
  krąży w `duration::ORBIT` (`repeat_synced`, wszystkie w fazie); Starting/Loading/
  Stopping — przerywany obrys przesuwa się w `CRAWL`; No status — nieruchome kreski.
- Znaczki: Needs you (żółty, trzy impulsy halo `PULSE` i stop, jeden przebieg
  `SWEEP` po wierszu), Error (czerwony „!”), Done (zielony ✓). Wejście znaczka:
  QUICK SmoothOut. Wycięcie wokół znaczka ma kolor tła wiersza (także na hover).
- Wiersze wchodzą z fade i 4 px, kolejne co STAGGER (maks. 5), przy rozwinięciu
  grupy i nowej sesji. Reduced Motion (`motion::policy`, odświeżane przy zmianie
  agentów) i `App::reduce_motion` pokazują stan końcowy bez pętli.
- Projekt ma monogram na stałym odcieniu z nazwy (`project_hue`, `components::project_tile`). Przy
  nazwie `właściciel/repozytorium` monogram i odcień liczą się z samego repozytorium.
  Worktree ma ikonę gałęzi (bieżący w akcencie) i tag `default` dla checkoutu repo.
- Akcje projektu/worktree/sesji są na hover (bieżący worktree i zaznaczona sesja
  stale) i zawsze w menu kontekstowym. Zwinięte grupy i stopka pokazują kropki
  Working/Needs you z liczbą i tooltipem.
- Animacja pętli odświeża okno tylko, gdy widoczna sesja pracuje lub startuje.
- Karty na hover (`sidebar/cards.rs`) zastępują wielolinijkowe tooltipy drzewa.
  Układ: tytuł, kropka + status z wyjaśnieniem (jedno zawijane zdanie), separator
  i fakty prowadzone ikoną zamiast etykiet.
  - Sesja: znak providera z agentem i modelem (CLI: „Terminal”), projekt, HEAD
    z tagiem `default`, folder, błąd Git; prywatny czat ma jedną linię „Private
    chat, no project”. Status czytany przy pokazaniu, więc karta śledzi agenta.
  - Worktree: gałąź (detached/unborn jak w Git) z tagami `default`/`locked`,
    „Current worktree”, Missing/Unavailable, projekt, folder, sesje
    (liczba, Working, Needs you) albo „Not opened yet”.
  - Projekt: monogram, liczba worktree (i gałęzi, gdy są), folder, sesje
    wszystkich worktree, błąd Git; bez repo „Folder without Git”/„Git unavailable”.
  Folder jest skrócony do `~/…` przy pierwszym otwarciu karty (nie w renderze
  sidebaru) i ucinany w środku, bo koniec ścieżki nazywa worktree.
- Projekty, worktree i sesje z innych komputerów rysują te same wiersze co lokalne
  (`ProjectHeader`, `WorktreeRow`, `SessionRow` w `sidebar/projects.rs` i
  `sidebar/chats.rs`); różni się tylko źródło stanu i akcji. Wiersz lokalny nie
  nazywa komputera. Zdalny worktree i zdalna sesja mają ikonę monitora przy nazwie,
  a sesja w drugiej linii status i nazwę komputera; karta ma fakt „On {komputer}”.
  Status zdalnego czatu pochodzi z katalogu hosta (`Activity::remote`), zdalny
  terminal ma kafelek terminala i stan Running/Exited. Gdy komputer jest offline
  lub się łączy, status sesji mówi „Offline”/„Connecting…”. Zdalne worktree
  zwijają się jak lokalne (`worktree_groups` po `remote:{host}:{workspace}`), a
  projekt udostępniany tylko przez inny komputer startuje zwinięty. Zdalne
  prywatne czaty stoją w Private chats zaraz po lokalnych, przed „Open tabs…”.
  Komputer bez udostępnionych projektów ma jeden zwarty wiersz: monitor, nazwa,
  kropka stanu i Connect, gdy jest offline; szczegóły i błąd połączenia są w karcie.
- Menu przy New chat ma sekcję „Other computers” z „New chat on {komputer}” dla
  połączonych komputerów. Czat powstaje w wybranym zdalnym worktree tego komputera,
  inaczej jako prywatny, z agentem, którego użyłby nowy czat tutaj (gdy host go ma),
  albo pierwszym agentem hosta (`remote_chat::new_chat_on`). Tooltip New chat
  nazywa komputer zaznaczonego zdalnego zasobu.
- Karta należy do całego wiersza i stoi przy krawędzi sidebara. Akcje wiersza
  i kropki liczników mają własne tooltipy, więc są `aside` (`hides_hover_card`):
  najechanie na nie chowa kartę zamiast pokazywać dwa dymki naraz.

## Karty na hover

`components::hover_card` to wspólny mechanizm kart z kilkoma faktami. `Button::tooltip`
przyjmuje tylko tekst, a natywny tooltip GPUI pojawia się przy kursorze i znika
bez animacji, więc karty mają własną warstwę.

- `HoverCards::install(window, cx)`: jedna warstwa na okno, właścicielem jest
  `Workspace`, który renderuje ją jako ostatnie dziecko (`deferred` + `Positioner`
  z gpui-base, priorytet nad tooltipami). Wyzwalacze znajdują ją przez okno.
- `HoverCardExt::hover_card(owner, placement, build)` na elemencie z id: karta
  obok elementu (sidebar: `Right`, composer i kolejka: `Top`). `build` działa przy
  każdym renderze widocznej karty, więc może czytać aktualny stan.
- `hides_hover_card()` na kontrolce wewnątrz wyzwalacza: karta chowa się, gdy
  kursor jest na kontrolce, i wraca po zejściu z niej.
- Intencja 500 ms przed pierwszą kartą (jak tooltipy GPUI). Gdy karta jest
  widoczna lub zniknęła mniej niż 300 ms temu, kolejny wyzwalacz otwiera się od
  razu, a karta płynie do niego z miejsca, w którym była (STATE_CHANGE).
  Kliknięcie i scroll zamykają ją bez okresu łaski.
- Ruch: `presets::TOOLTIP` — wejście QUICK EaseOut, wyjście MICRO EaseOut, bez
  opóźnienia zamknięcia. GPUI nie skaluje zwykłych elementów, więc skalę 0.98 z
  przepisu tooltipu zastępuje wysunięcie o `distance::MICRO` od wyzwalacza
  z fade. Reduced Motion pokazuje stan końcowy.
- Treść składamy z `hover_card::content` (`body`, `title`, `status`,
  `separator`, `facts`, `fact`, `error`), szerokość `t::hover_card_width()`.
- Karty w czacie: kółko kontekstu (miernik na każdy limit), odznaki `/komend`,
  `$skilli` i PR oraz wiadomości w kolejce.

## Zwijanie, resize i stałe szerokości paneli

Cmd+Option+B przełącza lewy sidebar (Windows: Ctrl+Shift+B), a Cmd+B panel
narzędzi (Windows: Ctrl+Shift+I). Lewy zachowuje Presence/PANEL. Prawy panel ma
własny trwały ToolPanel w Workspace, opcje Right/Bottom, resize i maksymalizację.
Wymiary są preferencją, a bieżący viewport ogranicza tylko wyświetlenie.
Ukrycie nie zamyka tabów/procesów. Jednoliniowy titlebar ma nazwę projektu i rozmowy.

`pane_divider` obsługuje lewy sidebar (180–800). `ToolResize` obsługuje panel
narzędzi (180–1600 szerokości lub 96–1600 wysokości). Preferowane wymiary
pozostają bez zmian, gdy viewport wymusza czasowe ograniczenie.

`Inspector` jest osobną encją, renderującą Agent / Changes / Tasks / Files.
Zmiana strony zachowuje niezależny crossfade treści (STATE_CHANGE), bez
odsłaniania pośrednich stron. Reduce Motion pomija animację. Skróty macOS
Cmd+Shift+S / Cmd+Shift+G oraz Windows Ctrl+Shift+A / Ctrl+Shift+G otwierają
odpowiednio agenta lub zmiany w panelu narzędzi.

`window_titlebar` zachowuje product chrome i natywne drag/double-click. Windows
dodaje trzy caption hit-test lanes Min/Max/Close z nazwami accessibility;
`HTMAXBUTTON` pozostawia Snap Layouts systemowi. Traffic-light padding występuje
wyłącznie na macOS, a prawa akcja workspace omija caption buttons.

## Korekta overscrollu FileTree

ScrollHandle w GPUI przyjmuje deltę koła, a clamp wykonuje później w prepaint.
Wirtualny zakres liczył się wcześniej z nieograniczonego offsetu i pomijał
wiersze, które po clamp nadal były na ekranie. Teraz zakres korzysta z
tego samego ograniczenia: 0..max(0, content_height - viewport_height).
Testy pokrywają oba końce krótkiej i długiej listy. GUI: powtarzany overscroll
w obu kierunkach nie usuwa wierszy. Dwa testy szerokości sprawdzają też,
że resize okna zmienia wyłącznie terminal, a toggle nie gubi szerokości.
Łącznie 23 testy przechodzą; Clippy i release również. Użytkownik potwierdził
ręczny resize obu sidebarów. Sprawdzone przełączenie na Changes w GUI.

## Taby i pane'y workspace

Elementy prezentacyjne znajdują się w `components/tabs.rs` i `components/pane.rs`:

| API | Rola |
| --- | --- |
| `tab_strip`, `workspace_tab`, `tab_drop_slot` | Pasek, pojedynczy tab i wolna strefa na końcu |
| `tab_host_badge` | Plakietka z monitorem i nazwą komputera po tytule taba zdalnego czatu (szerokość taba ją uwzględnia, nazwa do `TAB_BADGE_TEXT`) |
| `panel_loading` (`components/skeleton.rs`) | Spinner i jedna wyciszona linia, np. prawy panel, gdy zdalny workspace się łączy |
| `pane_surface`, `pane_header`, `pane_handle`, `pane_body` | Powierzchnia pane'a, nagłówek, uchwyt i obszar treści |
| `pane_drop_highlight(DropHighlight)` | Podświetlenie środka lub połowy docelowego pane'a |
| `split_surface`, `split_child` | Układ dwóch części zależny od PaneAxis |
| `resize_handle` | Wspólny obszar trafienia separatorów sidebarów i splitów |
| `DragPreview` | Prezentacyjna encja podglądu przeciągania |

Komponenty przyjmują etykiety, stan wizualny i elementy potomne; nie czytają
AppState, nie wykonują operacji modelu i nie znają TabId ani PaneId.
Zwracają Div/Stateful<Div>, więc caller może nadpisywać styl i dodawać eventy.
Marginesy centrujące separator na granicy splitu pozostają w miejscu użycia.
DragPreview jest encją wymaganą przez API GPUI, zawierającą wyłącznie etykietę.

`workspace_tabs.rs` łączy komponenty ze stanem i gestami tabów.
`workspace_panes.rs` odpowiada za geometrię drzewa, klasyfikację dropu oraz
wywołania operacji domenowych. Styl separatora jest wspólny z pane_divider,
który jest przezroczystym obszarem trafienia; linię rysuje właściciel regionu.

Po ekstrakcji: fmt, Clippy, 42 testy i build release przechodzą.
W uruchomionym release sprawdzono render zagnieżdżonych splitów, nagłówków,
uchwytów, separatorów i paska tabów. Nie wykonywano nowych pomiarów FPS
ani pełnego automatycznego przebiegu wszystkich gestów drag/drop.

## Modale i menu tabów

TextPrompt jest wspólnym modalem jednego pola tekstowego, opartym na
prezentacyjnym modal_surface. Używa elevated, control_border, input_bg i accent,
paddingu 20 px, odstępów 8/16 px oraz nagłówka 14 px. Właściciel przekazuje
tytuł, etykietę, wartość i callback walidacji/zapisu; komponent nie zna modelu taba.

Presence/POPOVER steruje otwarciem i zamknięciem, opacity powierzchni/backdropu
oraz przesunięciem distance::BASE. Reduce Motion jest respektowane.
Komponent jest usuwany dopiero po zakończeniu wyjścia. Nie używa domyślnej,
niezależnej animacji Dialog z GPUI Component.

Base Dialog zapewnia focus trap i izolację kliknięć. Enter zatwierdza, Escape,
Cancel i krzyżyk uruchamiają wyjście; backdrop nie zamyka formularza.
Skróty workspace są wyłączone na czas modala, a focus wraca do workspace po
wyjściu. Błąd walidacji pozostaje w formularzu; edycja czyści błąd.

Prawy przycisk na tabie otwiera Rename tab / Close tab. Operacje adresują
TabId i WorkspaceId klikniętego taba, także nieaktywnego. Zmiana projektu
w międzyczasie nie może zmodyfikować przypadkowego taba w nowym workspace.
Zmiana nazwy zachowuje pane'y i aktywny tab, a zamknięcie usuwa całe drzewo.
Własny tytuł jest częścią istniejącego snapshotu SQLite.

Weryfikacja: menu i modal w release, odrzucenie pustej nazwy, zapis przez
przycisk i Enter, focus po zamknięciu oraz zapis własnej nazwy w SQLite.
Test modelu obejmuje rename nieaktywnego taba ze splitami i zamknięcie
jego całego drzewa bez zmiany aktywnego sąsiada.

## Prezentacja projektów i formularzy modalnych

### Akcje w tle i stany przycisków

`ButtonLoading` należy do widoku. Handler przyjętej operacji i subskrypcja jej
wyniku wywołują `set(true/false, cx)` oraz notify właściciela. Render odczytuje
stan przez `loading_button`, `primary_loading_button` lub `loading_icon_button`.
`loading_button_with_icon` zachowuje istniejącą ikonę tekstowej akcji i zamienia
ją płynnie w loader w tym samym miejscu.
Wszystkie helpery zwracają natywny `Button`, zachowując Styled, focus, tooltipy
i callbacki. Nie włączaj loadera dla każdego disabled — walidacja i trwająca
operacja to różne stany. Przy zajętej własnej akcji używaj
`.disabled(unavailable && !feedback.active())`; native loading blokuje ponowne
kliknięcie i aktywację klawiaturą, zachowując czytelną akcję z loaderem.

Etykieta nie zmienia się na „Working…”: slot 16 + 4 jednostki logiczne rośnie
i maleje przez RESIZE, a loader zanika razem ze slotem. Przycisk o stałej
szerokości animuje zawartość; ikona lub Stop zachowują swój rozmiar i używają
crossfade. Przerwanie animacji zachowuje bieżący postęp, identyczny cel jej nie
restartuje. Reduce Motion pokazuje statyczny wskaźnik bez animacji; w spoczynku
komponent nie planuje klatek. Nazwa dostępności otrzymuje „in progress”.

Primary używa wariantu Primary i tokenów `button_primary` w `ui/theme.rs`.
Nie nadpisuj jego tła/foreground statycznym Styled tylko dla stanu normalnego:
GPUI Kit nakłada takie style również na disabled. Nieaktywny Primary ma teraz
stonowane tło i tekst, a pracujący przycisk zachowuje akcent oraz widoczny loader.

Wspólny backdrop używa `occlude()`: blokuje hover, tooltipy, kliknięcia i scroll
tła również podczas animacji wejścia/wyjścia. Popup pozostaje nad nim i zachowuje
swoje interakcje. Potwierdzenia mają początkowy focus wewnątrz modala.
`Confirmation::wait_for` obserwuje stan operacji, nie przejmuje jej zadania:
czeka z zamknięciem do sukcesu, po błędzie pozwala ponowić, a podczas pracy blokuje
Cancel/X/Escape. Worktree ma własny analogiczny cykl dla etapów usuwania.

Feedback podłączono do Git/worktree/Stop, zapisu issue i komentarzy, akcji Jira
(workflow, watch/vote, załączniki, usuwanie i historia), pobierania kolejnych
zadań/projektów/wartości oraz zapisu i sprawdzania konfiguracji w Preferences.
Stan należy do konkretnej operacji lub połączenia; blokada innej akcji nie
uruchamia wszystkich loaderów.

W izolowanym GUI sprawdzono warianty active/disabled/loading, błąd i ponowne
udostępnienie akcji, Enter/Escape oraz blokadę drugiego kliknięcia. Pomiar layoutu
przycisku z krótką etykietą pokazał szerokości 106–126 z 26 wartościami pośrednimi;
Reduce Motion dało tylko dwa stany, bez ciągłego odrysowywania. Backdrop zatrzymał
kliknięcie/hover licznika testowego i scroll listy, które działały po zamknięciu
modala. Testy nie wykonują zapisów do prawdziwych kont GitHub/Jira.
W głównej aplikacji modal worktree zablokował zmianę workspace'u i toggle
inspektora, a Escape przywrócił interakcje. Stan disabled sprawdzono również
na rzeczywistym przycisku Commit staged bez staged zmian.

- Nagłówki projektów i worktree używają Disclosure i standardowych przycisków.
- Pusty work area worktree (bez tabów) rysuje `Workspace::worktree_overview`
  (`src/ui/worktree_overview.rs`), opisany w „Przegląd worktree bez tabów” niżej.
- primary_button(id, label) — akcentowany wariant istniejącego button.
- components::modal: modal_surface, modal_header, modal_field, modal_actions,
  modal_backdrop — elementy prezentacyjne modala, konfigurowalne przez Styled.
- components/modal/text_prompt.rs — właściciel inputu, walidacji, focusu
  i Presence; elementy prezentacyjne nie posiadają tych stanów.
- components/modal/prompt.rs — renderer `Window::prompt` (`set_prompt_builder`)
  na platformach bez natywnych alertów (Linux); macOS i Windows zostają przy
  natywnych. Ten sam modal_surface/backdrop i Presence co Confirmation: Cancel
  po lewej, inne odpowiedzi dalej, `PromptButton::Ok` ostatni jako Primary;
  Esc wybiera Cancel, Enter — Ok. Odpowiedź idzie po animacji wyjścia.
- components/nested_scroll.rs — `.nested_scroll()` na przewijanym `Stateful<Div>`
  (z id): scroll chaining jak w przeglądarce. Kółko przewija element, a do
  przewijanych przodków (lista czatu) trafia dopiero, gdy element stoi już na
  krawędzi w tym kierunku. `ScrollHandle` trzyma `use_keyed_state`. Element
  dostaje wrapper (rozmiar, flex i marginesy przeniesione z elementu) z pionową
  `ScrollableMask` obok: działa w fazie capture, a narysowana przed maską
  przodka (np. listy czatu) rusza pierwsza, przewija, póki ma miejsce, i wtedy
  `stop_propagation`; na krawędzi przepuszcza zdarzenie dalej.

Odpowiednikiem class/merging w GPUI pozostaje Styled. Komponenty nie narzucają
zewnętrznych marginesów. Kontrolki input i button są współdzielone z resztą UI.
Po ekstrakcji przechodzą fmt, Clippy, 57 testów i release. W GUI sprawdzono
wiersz projektu, menu, wygląd formularza i zamknięcie przez Escape.

## Komponenty terminala

components/terminal.rs udostępnia terminal_surface(id), terminal_viewport(surface),
terminal_message(text) i process_status_bar(message, actions). Są to natywne
Div/Stateful<Div> z builderem Styled. Nie posiadają procesu, timera, focusu ani
danych emulatora. Pozycja komunikatu startowego i callbacki Restart/Close
pozostają w miejscu użycia; przyciski korzystają z istniejącego button().

TerminalView jest nadal jednym właścicielem stanu, rozdzielonym na moduły:
- mod.rs — pola i inicjalizacja;
- view.rs — kompozycja komponentów oraz podłączenie zdarzeń;
- lifecycle.rs — start, stop i odbiór zmian;
- viewport.rs — resize, centrowanie oraz geometria wspólna dla paint/myszy/IME;
- input.rs — klawiatura, schowek, IME i mysz;
- painter.rs — rysowanie siatki.

Ekstrakcja zachowuje identyfikatory pane'ów, kolejność operacji oraz reguły
lazy-start. Sprawdzenia: fmt, Clippy, 74 testy i build release.

## Preferences: granice modułów

- `preferences.rs`: okno, wspólny nagłówek, routing i przejście między stronami.
- `preferences/navigation.rs`: nawigacja i obserwacja wybranej strony.
- `preferences/general.rs`: General i jedna ścieżka synchronizacji opcji narzędzi.
- `preferences/tools.rs`: właściciel draftu, InputState i subskrypcji zapisu.
- `preferences/tools/view.rs`: nagłówek, wybór narzędzia, formularz i akcje.
- `preferences/tools/profiles.rs`: wiersze profili, ich pola i operacje na draftach.

Elementy prezentacyjne zwracają `Div`/`Button`, więc styl można nadpisać
builderem. Podział nie tworzy nowych encji pól ani subskrypcji w renderze;
profile nadal korzystają ze stanów utrzymywanych przez ToolsPreferences.

### Agent forms and tool headers

- Projekt i worktree mają jawne menu akcji. Kontrolki zachowują natywne
  focus/hover/disabled; etykiety branchy mają pierwszeństwo przed skrótami ikonowymi.

## Git Changes i diff

`ui::components::git` zawiera prezentacyjne buildery `diff_header`, `diff_line`,
`change_group`, `change_file_row`, `change_action` i `commit_form`.
Zwracają `Div`, `Stateful<Div>` lub `Button`, więc caller może nakładać Styled
oraz dopinać dzieci i callbacki. Wykorzystują istniejące przyciski i tokeny Canopy.

`DiffView` nadal zarządza ładowaniem i wirtualizacją diffu. `ChangesPanel` posiada
inputy, draft wiadomości, filtry, hover i operacje Git. Hover obejmuje wiersz,
a tooltip pozostaje na przycisku, aby nie nadpisywać listenera GPUI Kit.
Komponenty nie tworzą encji ani nie wykonują operacji Git podczas renderowania.

## Historia commitów

`ui::components::history` udostępnia `commit_row` i `commit_details` jako
natywne buildery Styled. Wewnętrzny `graph_lane` rysuje gotową geometrię grafu;
nie wylicza relacji Git podczas renderowania. Wysokość i odsłanianie szczegółów
wynikają z przekazanego progress, a kontrolkę zamknięcia dostarcza caller.

`HistoryView` pozostaje właścicielem paginacji, generacji żądań, zaznaczenia,
cache grafu oraz Presence/PANEL. Odstępy między listą, Load more i panelem,
w tym stały dolny padding 8 px, należą do widoku. Ekstrakcja nie dodaje
nowych encji, timerów ani listenerów hover.

`skeleton_bar()` jest statycznym builderem kształtu placeholdera, bez timerów
ani wymuszonych rozmiarów. `commit_skeleton` składa z niego wiersz dopasowany
do historii. Rozmiary i odstępy pozostają przy kompozycji.

## Referencje Git i synchronizacja

`ui::components::git_tracking` zawiera `reference_badge`, `ahead_behind`,
`upstream_membership` i `tracking_summary`. Komponenty zwracają `Div` z obsługą
Styled, bez operacji Git, nowych encji ani własnych timerów. Dane pochodzą ze
snapshotu historii; `None` dla membership pozostaje stanem nieznanym.
Lista commitów i szczegóły współdzielą etykiety referencji. Widok wybiera,
kiedy pokazać podsumowanie; jego stała wysokość zachowuje wyrównanie layoutu.

## Kontrolki transportu Git

`ui::components::git_network` zawiera `transfer_button`, `transfer_controls`
i `upstream_form`. Zwracają natywne buildery i korzystają z istniejących input,
dropdown, button oraz modal_field. Nie posiadają encji, zadań ani subskrypcji;
callbacki i formularz pozostają w ChangesPanel/UpstreamDialog. Zewnętrzne odstępy
należą do widoków, a wnętrze formularza zachowuje rytm modalu.

Zamykanie karty przez X, środkowy przycisk oraz menu kontekstowe trafia do
`Workspace::close_workspace_tab`, z kontrolą WorkspaceId i czyszczeniem hoveru.

## Toasty

`ui::components::toast` udostępnia `card`, `back_card`, `progress_bar` i
`close_button`. Wspólna powierzchnia zachowuje padding, tło i obramowanie obu
rodzajów kart. Buildery pozwalają nakładać Styled i podpinać zdarzenia.

`ui::toasts::state` zawiera testowalne odliczanie oraz politykę kolejki.
`ToastHost` zachowuje timer, generację anulowania, hover, gest swipe i stany motion;
przekazuje komponentom gotowe wartości animacji. Pozycja stosu w oknie pozostaje
w hoście. Ekstrakcja nie dodaje nowych encji ani timerów.


Pasek tabów używa tokenów `theme::tab_min_width()` / `tab_max_width()` / `tabs_height()` w rem i kontrolera
`workspace_tabs::TabScroll`. Menu wszystkich kart oraz przyciski są poza scrollerem.
Szerokość karty wynika z pomiaru etykiety i dostępnego miejsca; kontroler rezerwuje
miejsce na gradient, aby nie zakrywał przycisku zamknięcia. ScrollHandle oraz Transition
są trwałym stanem Workspace; zmiana aktywnego
TabId/WorkspaceId, indeksu, liczby kart, szerokości lub bazy fontu odsłania aktywną kartę.
W pełni widoczna karta nie przesuwa paska. Ręczny scroll przerywa przejście;
render nie odpytuje aktywnej karty w pętli i nie restartuje tego samego celu.

`tab_viewport` rysuje nieruchome cienie krawędzi nad przewijanym paskiem.
Ich siła wynika z aktualnego offsetu i max_offset ScrollHandle odczytanych
podczas paint, po clampie layoutu. Początek: cień prawy; środek: oba; koniec:
lewy; brak overflow: żadnego. Canvas nie przechwytuje kliknięć ani drag/drop
oraz nie dodaje timera czy osobnej pętli animacji.

## Session inspector

`ui/session_inspector.rs` owns the selected pane's session view and subscriptions
to agent/workspace changes. `ui/inspector.rs` renders the selected Agent/Changes/Tasks/Files tool page;
`ui/inspector/transition.rs` keeps its underline separate from page visibility.
Hook events notify the session view directly.

`components/session.rs` contains stateless builders:

- `session_status(label, color)` — status row and indicator.
- `session_info(label, value)` — aligned, truncated metadata row.
- `session_section(title)` — captioned container for questions, responses and activity.

All return `Div`, supporting native `Styled` overrides and additional children.
Outer spacing belongs to the caller. Data lookup, subscriptions and navigation
remain outside these presentation components. Notch continues to use the existing
`components/notch.rs` builders and its separate window/motion controller.

## Files and editor integration

The existing FileTree is fed by AppState.files; see [files-editor.md](files-editor.md).
EditorView wraps GPUI Kit's native Editor/EditorState. QuickOpen owns its input,
background search and local popup presence. No alternate input engine or CSS
class layer was introduced.

`components/files.rs` provides `file_message` and
`file_search_row`, all returning concrete Styled builders. Controllers supply
callbacks and surrounding spacing; search rows use stable path identities.
`components/controls.rs::code_editor` centralizes native Editor styling alongside
Input and Textarea. Search row/viewport dimensions are shared by presentation
and keyboard scrolling to keep selected results visible.

`tree_indent_guides(depth)` paints neutral, non-interactive ancestry lines inside
each virtualized Files row. TREE_INSET/TREE_INDENT are shared with row padding;
guides follow row motion and stop where the ancestor level ends. Nested levels
were visually checked in the release GUI.

`components/media.rs` provides `preview_surface`, `fitted_image`, `media_seek_bar`
and `playback_controls`. They return native Styled builders; spacing around the
preview, callbacks, seek geometry and asset/player lifetime remain with each view.
Font/image previews share contain sizing; video controls reuse existing buttons.

## Expanded task reader

`components/task_detail.rs` contains stateless native builders for the reader
header/title, tabs, description, metadata fields, footer, comment cards and
loading/empty states. `DetailLayout` computes the existing viewport bounds and
modal surface; `pages` receives the owner's transition progress. These components
return `Div`, `Stateful<Div>` or `Button`, so callers can apply `Styled` overrides
and attach actions. Width, separators and spacing around the metadata pane remain
at the call site. Existing input/button/icon/modal primitives are reused.

`ui/task_detail/mod.rs` retains focus, POPOVER/STATE_CHANGE, subscriptions and task
operations. `comments.rs` owns fetching, cancellation, cursor pagination and the
virtualized list. `comment_card.rs` owns each comment's Markdown entity and its
height-invalidation subscription. No workers, focus handles, timers or persistent
state are created by the presentation builders. The tab API uses `DetailTab`
instead of numeric presentation state; element IDs and transition positions are
unchanged.

## Task editing

`ui/task_edit` owns issue/comment forms, Markdown Write/Preview composition and
repository option pickers. It reuses the existing Input/Textarea/Button primitives.
The reader's metadata presenter accepts the editable metadata entity as a slot;
controllers retain write subscriptions and scope checks. Each comment keeps a stable
entity and mutable list index so editing/removal/reordering cannot invalidate another
comment's Markdown height. Draft persistence lives in `AppState.task_drafts`, and
remote writes live in `IntegrationsState`; neither is tied to a modal's lifetime.

`task_edit::metadata::MetadataEditor` owns the shared `TaskStatus` above each
provider's remaining metadata. It reuses native Select and ButtonLoading for
GitHub Open/Closed, Jira transitions and YouTrack state fields/events. The
`integrations::status` adapter supplies typed choices; only Jira transitions with
required fields emit a form request. Refresh/read generations and confirmed values
belong to TaskStatus; immediate writes never consume an issue-edit draft.

### Jira Tasks

`task_source::TaskSourcePicker` owns provider/site/project selectors and project
pagination. `preferences::jira::JiraPreferences` owns site/email/Cloud ID/token
inputs; GitHub preferences keep their independent form state. The provider heading
uses the CC0 Jira brand mark from the Electron reference.

`task_edit::TaskForm` selects a GitHub, Jira or YouTrack form inside the shared animated
TaskEditorDialog. `jira::form::JiraForm` owns schema requests, transition/type/board
selection, draft identity and writes; `jira::field::FieldInput` owns a single typed
field's native control, original value and rich-content mapping. `jira::panel`
owns Jira metadata/actions, attachment transfer and changelog reads.
`task_edit::metadata::MetadataEditor` delegates provider-specific fields to JiraPanel,
YoutrackPanel or the GitHub metadata editor. All forward domain events through the
same reader/editor modal lifecycle. A Jira transition requested from Status keeps
its selected ID and its own draft; stale metadata cannot choose a different action.

### Task browser controls and preview windows

`task_controls::TaskControls` owns quick project/filter selects, project discovery
and its preferences shortcut. `preferences::task_filters::TaskFiltersPreferences`
owns local filter drafts, CRUD and reveal motion. Definitions and selections belong
to the integration configuration, not to the UI or a Jira server saved-filter object.

`attachment_preview::AttachmentPreview` owns one read-only preview window. Its
native `attachment_native::NativePreview` owns QLPreviewView, the Escape/Cmd+W monitor
and a shared private `PreviewFile`; the library cache owns filesystem cleanup.
The bridge never receives credentials or remote URLs. See [task-browser.md](task-browser.md).

### Lazy filesystem tree

`FileTreeRequest` emits visible expanded directory IDs separately from `FileTreeEvent`
(open a file). `FileNode` carries ignored/loading/error flags. The Files owner coalesces
requests and builds nodes only from cached open levels; collapsed contents are released.
`TreeMotion` merges rows by stable identity during async insertion and preserves motion
on decoration changes. `QuickOpen` has its own cancellable search instead of consuming
an eager full-project index. See [files-editor.md](files-editor.md).

### Session scrolling and Markdown

`SessionInspector` owns a prepared selected-session snapshot, ScrollHandle and two
MarkdownView entities. One scroll viewport wraps an intrinsic-height, non-shrinking
content column. Metadata rows keep their geometry, truncate long values and expose
the full/raw values in tooltips. The inspector header remains outside this viewport.
`MarkdownView::live` retains the previous rendered response while its replacement is
prepared; changing pane/run clears it. `agents::presentation` maps raw tool/event IDs
for display only, including server context for MCP tools and configured profile names.

### Layout inspector (development only)

The bottom bar has a `braces` button right of Settings (`dev_tools_button` in `ui/mod.rs`) that toggles GPUI's
layout inspector (`window.toggle_inspector`, also ⌥⌘I from gpui-kit): pick an element to see its bounds and
edit its style live. It exists only where GPUI compiles the inspector — `debug_assertions` builds
(`cargo run`, `./scripts/build-macos.sh dev`) or the `dev-inspector` feature; release and profiling builds
have neither the button nor the shortcut. Picking works like a browser's: GPUI picks the frontmost element under the pointer, and
gpui-kit's `Root` always draws invisible full-window layers over the app (notifications, tooltips, menus), so
`ui/dev_inspector.rs` replaces gpui-kit's inspector panel (`set_inspector_renderer`) and, while picking, steps
past a full-window gpui-kit element under the pointer with GPUI's own pick-depth wheel step; the style editors
are still gpui-kit's (`render_inspector_states`). Scrolling while picking still moves the depth by hand.

## Wspólny nagłówek i responsywny workspace

Kategorie narzędzi należą do `workspace_layout::tool_navigation`, montowanego
w nagłówku okna. Tylko kontekst jest obszarem drag; przyciski nie dziedziczą
hitboxa przeciągania. Lokalny pasek zawiera wyłącznie karty i ich akcje.
Sidebar ma osobne scrolle projektów i Private chats, z przypiętymi nagłówkami.
Wymuszone ograniczenia rozmiaru są prezentacją, nie mutacją preferencji.
Nakładka wąskiego sidebara używa `gpui_kit::base::FocusTrapElement` i przywraca
poprzedni fokus. Kontrakt: [workspace-layout](features/workspace-layout.md).

## Przegląd worktree bez tabów

Gdy wybrany worktree (albo przestrzeń Private chats) nie ma żadnego taba, work
area pokazuje `Workspace::worktree_overview` (`src/ui/worktree_overview.rs`)
zamiast samego CTA. Treść jest wyśrodkowana w obu osiach marginesami `auto`
w przewijanym kontenerze, więc w niskim oknie przewija się od góry zamiast
ucinać. Bez motion.

- Nagłówek: kafelek projektu (monogram i odcień jak w sidebarze), nazwa
  `właściciel/repozytorium` z origin (inaczej nazwa folderu repo), HEAD z ikoną
  gałęzi i tagami `default`/`locked`; bez Git — „Folder without Git”,
  „Git unavailable”, „Reading Git…”.
- Fakty: folder (`~/…`, ucinany w środku) z Copy path; dla dodatkowego worktree
  „Created from {branch}” z `Project::worktree_base`.
- Akcje: New chat (primary, wyłączony z powodem `Chats::unavailable` pod
  akcjami), Open terminal (tylko z projektem), Resume chat… (paleta `/resume`).
- Chats: czaty katalogu bez taba w tym workspace (`Chats::detached`, np.
  utworzone zdalnie) ze znakiem agenta i statusem `navigation_status`;
  kliknięcie otwiera tab (`open_detached`). Sekcja znika, gdy ich nie ma.
- Files / Changes / Tasks: ghost przyciski otwierające stronę panelu narzędzi
  (`select_tool_page`), tooltip ze skrótem akcji.
- Private chats: tytuł i jedno zdanie, New chat i Resume chat…; bez folderu,
  terminala i narzędzi projektu.

Dane pochodzą wyłącznie z istniejących modeli: `ProjectsState.catalog`,
`GitState` (worker libgit2), `Chats`, `ToolsState`. Katalog domowy jest
odczytywany raz na proces (`OnceLock`). Liczba zmian, ahead/behind i ostatni
commit nie są pokazywane: `ChangesState` obserwuje repo tylko przy widocznym
Changes, a historia należy do `HistoryView`, więc w tym miejscu byłyby nieaktualne.
`Checkout::of`, `home_relative`, `path_label` i `agent_mark` są współdzielone ze
szczegółami pustego czatu (`ui/chat/details.rs`). Monogram (`project_tile`) i tag
`default`/`locked` (`checkout_tag`) pochodzą z `components/identity.rs`, tak samo jak
w sidebarze i jego kartach.
