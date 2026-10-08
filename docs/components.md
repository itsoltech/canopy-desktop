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
| `input(&state)` | Wspólna baza pola tekstowego; wariant osadzony przez `.appearance(false)` | `Entity<InputState>` w widoku |
| `dropdown(&state)` | Select z dowolnym delegatem GPUI, wspólny trigger i menu | `Entity<SelectState<D>>` w widoku |
| `checkbox(id, checked, label)` | Checkbox z nazwą dostępności | bool w widoku |
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
Projects, Files i Tools mają niezależne stany w encji Sidebar. Po zamknięciu
sekcji nie usuwamy encji FileTree, więc jej zaznaczenie i rozwinięcia pozostają.

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

## Zwijanie, resize i stałe szerokości paneli

Przycisk przy traffic lights oraz Cmd+Option+B przełączają lewy sidebar na
macOS; Windows używa Ctrl+Shift+B. Cmd+B/Ctrl+Shift+I przełączają prawy.
Oba używają Presence/PANEL i zachowują
encje oraz szerokości po ponownym otwarciu. Lewa zawartość jest zakotwiczona
z lewej, prawa z prawej: rozszerzający się terminal zasłania je przez clip,
a zawartość nie wyjeżdża poza okno.

`state/layout.rs` przechowuje preferowane szerokości. Zmienia je wyłącznie
przeciąganie separatora, nigdy resize okna ani klatka animacji. Usunięto
proporcjonalny ResizableState oraz pętlę synchronizacji, która mogła cofać
ręczny resize lewego panelu. Terminal dostaje pozostałą szerokość.

`pane_divider` to wspólny uchwyt z obszarem trafienia 8 px i cienką linią.
Przeciągnięcie przekazuje PanelSide; Workspace aktualizuje właściwą szerokość.
Limity: lewy 180–800, prawy 220–800 jednostek logicznych; budżet zależy od
aktualnego okna i pozostawia co najmniej 160 dla terminala. Ukryty panel nie
zajmuje budżetu. Szerokości są zapisywane w snapshotcie sesji. Jeżeli po resize
okno jest za małe, tylko wyświetlane szerokości są tymczasowo ograniczane;
powiększenie okna przywraca preferowane wartości. Dopasowana szerokość dotyczy
również zawartości panelu, żeby kontrolki nie były obcięte poza viewportem.
Separatory nie są montowane podczas animacji toggle.

`Inspector` jest osobną encją. Session / Changes / Tasks mają przesuwany znacznik
(RESIZE) oraz niezależny crossfade treści (STATE_CHANGE). Przeskok Session ↔ Tasks
nie odsłania Changes: widoczność jest animowana dla konkretnych wybranych stron,
a nie wyliczana z pozycji znacznika. Szybkie zmiany zachowują aktualne opacity;
wychodzące strony pozostają do końca animacji, z blokadą kliknięć i scrollu treści.
Reduce Motion przełącza oba kanały natychmiast. Skróty macOS
Cmd+Shift+S / Cmd+Shift+G oraz Windows Ctrl+Shift+A / Ctrl+Shift+G wybierają tab
i rozwijają panel, jeśli był zamknięty.

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
który nadal zachowuje linię sidebara i pusty podgląd resize.

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

- Nagłówki projektów używają Disclosure i hover_action; worktree korzystają
  z list_button bez tła, z gwiazdką wskazującą aktywny katalog.
- empty_state(action, status, error) — centralny pusty stan; caller odpowiada
  za dostępność akcji, komunikaty oraz motion/pozycjonowanie.
- primary_button(id, label) — akcentowany wariant istniejącego button.
- components::modal: modal_surface, modal_header, modal_field, modal_actions,
  modal_backdrop — elementy prezentacyjne modala, konfigurowalne przez Styled.
- components/modal/text_prompt.rs — właściciel inputu, walidacji, focusu
  i Presence; elementy prezentacyjne nie posiadają tych stanów.

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

- `ToolHeader::button`: wspólna prezentacja strzałki, ikony, nazwy i liczników.
  Właściciel wybiera akcję (rozwiń lub uruchom) i przekazuje stan hoveru.
- `hover_action(id, button, callback)`: osobny hitbox dla hoveru. Tooltip pozostaje
  na przycisku wewnątrz. To celowe: GPUI Kit instaluje tooltip przez `on_hover`,
  nadpisując wcześniejszy listener na tym samym elemencie. Wymiary nadaje caller.
- `SelectOption`: stabilna wartość i niezależna etykieta dla natywnego Select.
  Wspólne dla General i formularzy agentów.
- `preferences/agent/options.rs`: pojedyncza definicja opcji używana podczas
  tworzenia kontrolek i zmiany profilu. `state.rs` obsługuje ładowanie/zbieranie
  draftu, `view.rs` sekcje formularza, a `environment.rs` wiersze zmiennych i
  formularz dodawania. Encje i subskrypcje zachowują dotychczasowych właścicieli.

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


Pasek tabów używa wspólnego `TAB_WIDTH` i kontrolera `workspace_tabs::TabScroll`.
ScrollHandle oraz Transition są trwałym stanem Workspace; zmiana aktywnego
TabId/WorkspaceId, indeksu, liczby kart lub szerokości odsłania aktywną kartę.
W pełni widoczna karta nie przesuwa paska. Ręczny scroll przerywa przejście;
render nie odpytuje aktywnej karty w pętli i nie restartuje tego samego celu.

`tab_viewport` rysuje nieruchome cienie krawędzi nad przewijanym paskiem.
Ich siła wynika z aktualnego offsetu i max_offset ScrollHandle odczytanych
podczas paint, po clampie layoutu. Początek: cień prawy; środek: oba; koniec:
lewy; brak overflow: żadnego. Canvas nie przechwytuje kliknięć ani drag/drop
oraz nie dodaje timera czy osobnej pętli animacji.

## Session inspector

`ui/session_inspector.rs` owns the selected pane's session view and subscriptions
to agent/workspace changes. `ui/inspector.rs` owns the Session/Changes/Tasks switch;
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
