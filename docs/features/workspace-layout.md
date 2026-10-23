# Nawigacja i obszar narzędzi worktree

Lewy sidebar ma jedno drzewo: projekt → worktree → rozmowy. Poniżej pozostaje
sekcja Private chats. Wiersz rozmowy pokazuje tytuł, agenta i tekstowy status;
kontekst projektu/HEAD jest na rodzicu i w tooltipie. Zwijanie nie uruchamia
ani nie zatrzymuje sesji. Rodzice pokazują liczbę pracujących agentów i rozmów
wymagających uwagi. Kolory statusów, zaznaczenia i marki pochodzą z `ui/theme.rs`.

Kliknięcie worktree przywraca jego ostatni widok. Przycisk terminala przy
worktree otwiera wybraną istniejącą sesję terminala (lub pierwszą dostępną),
a dopiero przy jej braku tworzy Shell. Plus przy worktree tworzy w nim rozmowę.
Globalne New chat nadal używa aktywnego worktree; prywatny czat wybiera się
jawnie w menu. Sam wybór projektu nie tworzy procesu.

## Centrum i narzędzia

Górny pasek ma jedną linię: projekt i tytuł rozmowy. Nie ma osobnej belki ze
ścieżką ani drugiego tytułu nad treścią. Centrum wyświetla wybraną rozmowę.
Pusty stan ma New chat. Rozmowy w istniejącym wspólnym splicie pozostają razem.

Panel narzędzi jest po prawej, z opcją Dock at bottom, maksymalizacją i resize.
Ma własny wybór taba. Wybór innej rozmowy w tym samym worktree zachowuje
wybrany terminal, jego fokus i podziały. Plus tworzy kolejny terminal;
Split dzieli wybraną sesję i tworzy niezależny PTY. Menu ma także pionowy split.
Każdy pane ma przycisk zamknięcia; karty zachowują rename, close, reorder i dock.
Nowe shelle dostają rozróżnialne nazwy Terminal 1, Terminal 2 itd.

Menu panelu udostępnia Files, Changes, Tasks, Agent oraz narzędzia/profile.
Nie ma ich drugiej kopii po lewej. Files używa istniejącego FileTree i edytorów,
Tasks i Changes dotychczasowych paneli. Agent śledzi centralną rozmowę również
przy fokusie terminala. Klik nazwy strony narzędzi wraca do tabów terminala.
Files/Changes i tworzenie nowych narzędzi wymagają projektu; Agent/Tasks
pozostają dostępne dla rozmów prywatnych. Błąd uruchomienia jest widoczny w panelu.

Ukrycie panelu zachowuje wszystkie sesje. Zamknięcie taba/pane'a przechodzi przez
dotychczasowy guard i cleanup. Skróty splitu dotyczą wybranego terminala;
Cmd+T zachowuje wybór domyślnego narzędzia z ustawień. Cmd+B przełącza panel,
Cmd+Option+B lewy sidebar (Windows: dotychczasowe odpowiedniki).

## Stan i zgodność

`Workspace` zachowuje te same taby i drzewa. Tab zawierający `PaneKind::Chat`
jest centralny, pozostałe taby trafiają do narzędzi. Mieszanych drzew nie
rozdzielamy, a identyfikatory pane'ów i historii pozostają bez zmian.
`last_chat` i `last_tool` zapamiętują oba wybory. `panel` przechowuje stronę,
widoczność, położenie, preferowaną szerokość/wysokość i maksymalizację.
Właścicielem jest konkretny workspace, więc ustawienia nie przechodzą między worktree.

Nowe pola mają wartości domyślne przy odczycie starszych snapshotów. Nie ma
migracji SQLite ani odtwarzania procesu na podstawie nazwy taba. Plan aktywacji
obejmuje tylko widoczne drzewo rozmowy i wybrany widoczny tab narzędzi.
Zasoby wcześniej uruchomione żyją do jawnego zamknięcia, nawet gdy ich panel
jest schowany. Przy resize okna zmienia się geometria wyświetlenia, bez
nadpisywania preferowanych wymiarów. Starsze pola inspektora w LayoutState
pozostają czytelne; nowy panel korzysta z ustawień workspace'u.

## Weryfikacja

Testy modelu sprawdzają niezależny wybór, zachowanie splitów, lazy-start,
ukrywanie bez zamykania i roundtrip oraz odczyt starszego snapshotu. Testy
headless GPUI używają prawdziwego sidebara i okna Workspace: zagnieżdżanie,
zwijanie, klawiatura, split, maksymalizacja i przeniesienie panelu na dół.
Nie zastępują ręcznego sprawdzenia wyglądu natywnego okna ani E2E.
