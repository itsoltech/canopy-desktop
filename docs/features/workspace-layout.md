# Nawigacja i obszar narzędzi worktree

Lewy sidebar ma drzewo projekt → worktree → sesje oraz osobne Private chats.
Sesja ma wiersz 2.75rem: kafelek providera (znak Claude/OpenAI zamiast nazwy agenta,
nazwa w karcie sesji na hover i etykiecie dostępności), tytuł 14/20 z glifem terminala dla CLI
oraz status 12/16 z modelem („Working · Opus 5.5”), skalowane przez rem. Working
jest neutralną etykietą, decyzje/błędy używają kolorów semantycznych; Starting nie
zwiększa licznika pracy. Po braku telemetry przez czas handshake pojawia się
No status, również w Agent. Nie rozpoznajemy stanu przez tekst TUI. Wygląd stanów
i ruch: [komponenty](../components.md#sidebar-tożsamość-i-aktywność-agentów).

New chat i nagłówek Projects pozostają nad przewijanym drzewem. Private chats ma
własną listę o ograniczonej wysokości i nagłówek poza przewijaniem. Przy pierwszym
pokazaniu otwarty jest aktywny projekt/worktree; pozostałe grupy można rozwinąć.
Zmiana aktywnego worktree odsłania jego rodziców, a samo zwinięcie nie zmienia
wybranej pracy. Zaznaczenie ma tło i pasek, rodzic wskazuje kontekst kolorem nazwy.
Agregaty pracy/uwagi są w jednym wierszu zwiniętego rodzica. Projekt, worktree
i sesja mają karty na hover z faktami (folder, gałąź, sesje).

Kliknięcie worktree przywraca ostatni widok. Przycisk terminala przy worktree
przywraca istniejącą sesję, a dopiero przy jej braku tworzy Shell. Menu worktree:
New chat, Open terminal, Stop processes, Remove worktree. Menu projektu:
New worktree, Refresh Git metadata, Close project. New chat używa aktywnego
worktree, a prywatną rozmowę wybiera się jawnie. Wybór projektu i resize okna
nie tworzą procesów. Błędy pozostają przy odpowiednich funkcjach.

## Wybór sesji, fokus i terminale

`selected_pane` identyfikuje pracę wybraną w nawigacji. Fokus klawiatury, lokalna
karta narzędzi i maksymalizacja są niezależne. Agent zawsze nazywa analizowaną
sesję. Fokus terminala lub powiększenie panelu nie wybiera innej rozmowy.

Centrum ma pasek tabów jak przed przebudową: rozmowy, terminale, CLI i pliki
dzielą jeden pasek, a pod nim jest drzewo pane'ów aktywnego taba. Wybranie sesji
(także CLI) w drzewie aktywuje jej istniejący tab i pane, bez nowego PTY. Panel
narzędzi nie ma strony terminali; zachowuje swoją stronę i widoczność przy zmianie
taba. Ukryte taby zachowują procesy.

Mieszane drzewa zachowują wszystkie pane'y. Zamknięcie uruchamia dotychczasowy
guard i cleanup. Karty zachowują rename, close, reorder i dock. Plus na pasku
tabów tworzy nowy terminal z kolejną nazwą; Split dzieli terminal aktywnego taba
i tworzy niezależny PTY. Ukrycie narzędzi nie zmienia tabów. Pusty czat pokazuje pomoc nad
composerem, bez odsyłacza sugerującego kontynuację u innego agenta.

## Nagłówek i geometria

Belka okna zawiera projekt/worktree, tytuł wybranej pracy i przełącznik panelu
narzędzi. Kategorie Files, Changes, Tasks, Agent są na górze samego panelu
(na wysokości paska tabów), więc schowany panel nie pokazuje ich wcale; otwiera
go przycisk w belce albo skrót kategorii. Kategorie pokazują stronę narzędzi
idempotentnie; Hide jest osobnym poleceniem. Wąski panel zachowuje nazwę
aktywnej kategorii i menu Tools z zaznaczoną stroną. Tytuł pracy ma pierwszeństwo
przed gałęzią i projektem. Pasek tabów w centrum przewija się,
odsłania aktywną kartę wraz z przyciskiem zamknięcia, ma cienie krawędzi i menu
wszystkich kart. Szerokości są mierzone z etykiet (7–18rem i limit viewportu). Akcje pozostają
nieruchome. Jedna granica wizualna należy do jednego regionu.

Obszar drag/double-click obejmuje kontekst belki, nie jej przyciski. Settings
jest w stopce sidebara i w Tools; globalny pusty status bar został usunięty.
Files/Changes wymagają projektu; Agent i Tasks są dostępne także prywatnie.
Skróty: Option+Cmd+1 wraca do tabów, Option+Cmd+2…5 to kategorie panelu
(Ctrl+Alt+… na pozostałych platformach).
Istniejące skróty paneli pozostają; Linux ma Ctrl+N / Ctrl+Shift+N dla tworzenia
rozmowy / wyboru miejsca.

Panel jest domyślnie po prawej, z opcją Dock at bottom. W poziomym splicie
rozmowa ma minimum 28rem, narzędzia 24rem, sidebar 15rem. Gdy split się nie
mieści, widoczny jest jeden obszar; powrót nazywa rozmowę. Przy braku miejsca
na sidebar + treść sidebar jest czasową nakładką, z własnym fokusem i focus trap, przyciemnieniem i cieniem.
Escape lub zamknięcie przywraca fokus. Wymiary i preferencje widoczności nie są
nadpisywane przez dopasowanie do okna. Pusty worktree ma wycentrowany kontekst,
New chat i Open terminal, bez automatycznego tworzenia sesji.

## Persist i weryfikacja

`last_chat`, `last_tool`, `selected_pane`, `preview` oraz `panel` należą do workspace'u.
Brak nowych pól w starszym snapshotcie jest poprawny; cel jest wyliczany z
istniejących tabów, nigdy z geometrii. Nie ma migracji SQLite ani zmiany PaneId.

Testy modelu obejmują wspólny pasek tabów, niezależność panelu od taba, wybór CLI,
pierwszy czat po CLI i restore. Testy GPUI obejmują menu, split/dock, sidebar, szerokości i bazy
16/18/20 oraz fokus nakładki. `ui::visual_tests::native_visual_fixture` jest jawnie
ignorowaną sceną do odbioru natywnego: używa produkcyjnych widoków, danych
syntetycznych i deterministycznego bufora terminala, bez modeli, PTY ani kont.
Zmienne CANOPY_UI_SCENE, CANOPY_UI_WIDTH/HEIGHT/FONT wybierają scenę i geometrię.
To odbiór renderowania i interakcji, nie dowód działania usług ani kwalifikacja
macOS/Windows. Nie zastępuje E2E.
