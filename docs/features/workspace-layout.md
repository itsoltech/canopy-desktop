# Nawigacja i obszar narzędzi worktree

Lewy sidebar ma drzewo projekt → worktree → sesje oraz osobne Private chats.
Sesja zajmuje dwie linie: tytuł 14/20 oraz agent i tekstowy status 12/16, skalowane
przez rem. Marka agenta i status mają osobne znaczenia. Working jest neutralną
etykietą, decyzje/błędy używają kolorów semantycznych; Starting nie zwiększa
licznika pracy. Po braku telemetry przez czas handshake pojawia się No status,
również w Agent. Nie rozpoznajemy stanu przez tekst TUI.

New chat i nagłówek Projects pozostają nad przewijanym drzewem. Private chats ma
własną listę o ograniczonej wysokości i nagłówek poza przewijaniem. Przy pierwszym
pokazaniu otwarty jest aktywny projekt/worktree; pozostałe grupy można rozwinąć.
Zmiana aktywnego worktree odsłania jego rodziców, a samo zwinięcie nie zmienia
wybranej pracy. Zaznaczenie ma tło i pasek, rodzic wskazuje kontekst kolorem nazwy.
Agregaty pracy/uwagi są w jednym wierszu zwiniętego rodzica.

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

Wybranie CLI w drzewie pokazuje jego istniejący tab/drzewo, bez nowego PTY.
Tymczasowe `preview` zachowuje wcześniejszą widoczność i stronę narzędzi;
`last_tool` nadal wskazuje narzędzie przypisane do układu rozmowy. Sekwencja
A + shell S → CLI B → A wraca do A + S. Jawny wybór karty narzędzi zmienia
zapamiętany tab. Pierwszy czat po pracy wyłącznie w CLI otwiera się bez
przypadkowego terminala B obok; B pozostaje dostępny i nie jest zamykany.

Mieszane drzewa zachowują wszystkie pane'y. Zamknięcie uruchamia dotychczasowy
guard i cleanup. Karty zachowują rename, close, reorder i dock. Plus tworzy nowy
terminal z kolejną nazwą; Split dzieli wybraną sesję i tworzy niezależny PTY.
Ukrycie narzędzi zachowuje ich sesje i podziały. Pusty czat pokazuje pomoc nad
composerem, bez odsyłacza sugerującego kontynuację u innego agenta.

## Nagłówek i geometria

Jedna belka zawiera projekt/worktree, tytuł wybranej pracy oraz kategorie
Terminal, Files, Changes, Tasks, Agent. Kategorie pokazują stronę narzędzi
idempotentnie; Hide jest osobnym poleceniem. Wąski nagłówek zachowuje nazwę
aktywnej kategorii i menu Tools z zaznaczoną stroną. Tytuł pracy ma pierwszeństwo
przed gałęzią i projektem. Terminal pokazuje liczbę
pane'ów terminalowych, także schowanych. Oddzielny pasek kart przewija się,
odsłania aktywną kartę wraz z przyciskiem zamknięcia, ma cienie krawędzi i menu
wszystkich kart. Szerokości są mierzone z etykiet (7–18rem i limit viewportu). Akcje pozostają
nieruchome. Jedna granica wizualna należy do jednego regionu.

Obszar drag/double-click obejmuje kontekst, nie kategorie ani menu. Settings
jest w stopce sidebara i w Tools; globalny pusty status bar został usunięty.
Files/Changes wymagają projektu; Agent i Tasks są dostępne także prywatnie.
Skróty kategorii: Option+Cmd+1…5 na macOS, Ctrl+Alt+1…5 na pozostałych platformach.
Istniejące skróty paneli pozostają; Linux ma Ctrl+N / Ctrl+Shift+N dla tworzenia
rozmowy / wyboru miejsca.

Panel jest domyślnie po prawej, z opcją Dock at bottom. W poziomym splicie
rozmowa ma minimum 28rem, narzędzia 24rem, sidebar 15rem. Gdy split się nie
mieści, widoczny jest jeden obszar; powrót nazywa rozmowę. Przy braku miejsca
na sidebar + treść sidebar jest czasową nakładką, z własnym fokusem i focus trap, przyciemnieniem i cieniem.
Escape lub zamknięcie przywraca fokus. Wymiary i preferencje widoczności nie są
nadpisywane przez dopasowanie do okna. Pusty worktree ma wycentrowany kontekst,
New chat i Open terminal / Show terminals, bez automatycznego tworzenia sesji.

## Persist i weryfikacja

`last_chat`, `last_tool`, `selected_pane`, `preview` oraz `panel` należą do workspace'u.
Brak nowych pól w starszym snapshotcie jest poprawny; cel jest wyliczany z
istniejących tabów, nigdy z geometrii. Nie ma migracji SQLite ani zmiany PaneId.

Testy modelu obejmują powrót z CLI, niezależność fokusu, pierwszy czat po CLI i
restore. Testy GPUI obejmują menu, split/dock, sidebar, szerokości i bazy
16/18/20 oraz fokus nakładki. `ui::visual_tests::native_visual_fixture` jest jawnie
ignorowaną sceną do odbioru natywnego: używa produkcyjnych widoków, danych
syntetycznych i deterministycznego bufora terminala, bez modeli, PTY ani kont.
Zmienne CANOPY_UI_SCENE, CANOPY_UI_WIDTH/HEIGHT/FONT wybierają scenę i geometrię.
To odbiór renderowania i interakcji, nie dowód działania usług ani kwalifikacja
macOS/Windows. Nie zastępuje E2E.
