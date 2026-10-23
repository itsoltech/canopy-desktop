# Persist i restore workspace'u

Źródło: `state/session.rs`, operacje SQLite w `settings/database.rs`,
koordynacja UI w `app_state/projects.rs`.

## Format i zakres

Tabela `_canopy_rust_session`: singleton id=1, version=1, payload JSON.
Snapshot zawiera:

- uporządkowany katalog projektów, ProjectId i aktywny WorkspaceId;
- ścieżkę konkretnego worktree, opcjonalny `repository_path` grupujący
  workspace’y repozytorium oraz opcjonalną bazę utworzenia (`ref` i OID),
  używaną wyłącznie jako podpowiedź porównania przy usuwaniu
  (szczegóły: [git-worktrees.md](git-worktrees.md));
- workspace każdego projektu, uporządkowane taby i aktywny TabId;
- przestrzeń samodzielnych rozmów w opcjonalnym `scratch`, również przy aktywnym projekcie;
  Chat PaneId trafia do rodzica worktree albo Private chats; pozostałe taby scratch
  pozostają dostępne przez Open tabs…;
- całe drzewo każdego taba: kolejność first/second, oś, SplitId i proporcje;
- PaneId oraz focused PaneId;
- metadane pane'a: tool ID, rodzaj widoku, cwd, profil, tytuł, resource,
  argumenty narzędzia i opcjonalny identyfikator wznowienia sesji providera;
- opcjonalny `PaneMetadata.chat_title_state` (placeholder/named), bez zmiany schematu SQL;
  brak pola oznacza starszą nazwę czatu z fallbackiem do taba. Nazwy są utrwalane przy
  mutacji, nie przy odczycie; dock/swap/detach nie gubią tytułu rozmowy;
- szerokość/widoczność lewego sidebara; per workspace: wybór rozmowy i narzędzia,
  widoczność, strona, położenie, wymiary i maksymalizacja panelu narzędzi.
  Starsze pola inspektora LayoutState pozostają czytelne.

Identyfikatory przeżywają restart. Deserializacja rezerwuje odczytane wartości
w generatorach, aby nowe taby/pane'y nie kolidowały z odtworzonymi.
Nie zapisujemy PID-ów, uchwytów PTY ani flag „running”. Nie ma jeszcze
serializacji dokumentów edytora, bufora terminala, uchwytów okien czy geometrii
okna macOS — nie są częścią bieżącego modelu workspace.

Walidacja odrzuca m.in. duplikaty tożsamości tabów/pane'ów, brak celu fokusu,
niewłaściwy aktywny tab/projekt, niepoprawne proporcje i ścieżki cwd oraz
niezgodne powiązania projektu z workspace'em. Limity: 128 projektów,
256 tabów na workspace, głębokość splitu 4 i payload 8 MiB.
Nowsza wersja i uszkodzony zapis nie są automatycznie nadpisywane.
Pole bazy worktree ma `serde(default)`, więc starsze sesje pozostają zgodne;
istniejące nazwy katalogów i layouty nie są migrowane ani przepisywane.

## Zapis

Obserwacje encji Workspace i LayoutState oraz operacje katalogu projektów
oznaczają zmianę snapshotu. Jeden writer scala zmiany co 200 ms; SQLite
i serializacja działają na workerze. Numer generacji wymusza kolejny zapis,
jeśli stan zmienił się podczas poprzedniego. Nie tworzymy zadania per klatka.
Identyczny stan nie wywołuje kolejnego zapisu.

Session i starszy indeks `_canopy_rust_projects` zapisują się w jednej transakcji.
Po pojawieniu się pełnej sesji stare API save_projects odrzuca zapis samej
listy — do zmian służy save_session. Odczyt starszej bazy zawierającej tylko
listę projektów nadal działa i tworzy początkowe workspace'y.

Zmiana jest widoczna w pamięci od razu; błąd zapisu zachowuje ostatni poprawny
snapshot na dysku i pokazuje komunikat o niezapisanym layoucie. Kolejna zmiana
lub ponowna próba zamknięcia ponawia zapis.

⌘Q/zamknięcie ostatniego okna czeka na operację projektu, writer i końcowy
snapshot pobrany z bieżących encji. Błąd końcowego zapisu pozostawia aplikację
otwartą. Nagłe ubicie procesu może stracić zmiany z bieżącego okna scalania;
nie obiecujemy gwarancji przy awarii zasilania lub force kill.

## Odtwarzanie i leniwy start narzędzi

reopenLastWorkspace=false pozostawia zapis nietknięty i pokazuje pusty ekran.
Przy true najpierw czytamy pełną sesję, a dopiero przy jej braku starszą listę.

Odtwarzamy wszystkie modele bez uruchamiania procesów. Niedostępny katalog
pełnej sesji daje komunikat, ale nie usuwa jego zapisanego drzewa pane'ów.
Przełączenie projektu zachowuje odtworzone workspace'y w pamięci.

`SessionSnapshot::activation_plan()` obejmuje aktywny workspace. Jego plan zawiera
pane'y aktywnego taba (o ile narzędzia nie są zmaksymalizowane). Niewidoczne taby nie
są uruchamiane, ale uruchomione zasoby nie są zatrzymywane przez zmianę widoczności.
`last_chat`, `last_tool`, `selected_pane`, `preview` i `panel` mają wartości domyślne
przy odczycie starszych zapisów. `selected_pane` jest wyborem pracy niezależnym
od fokusu. `preview` pochodzi ze starszych zapisów (CLI pokazany w panelu): przy
odczycie przywraca tylko wcześniejszą widoczność/stronę panelu i nie jest już tworzony.
Strona `Tabs` ze starszych zapisów oznacza schowany panel. Resize i nakładka sidebara nie zapisują wymuszonych
wymiarów ani widoczności.
Taby, splity i PaneId nie są migrowane. Szczegóły: [układ](features/workspace-layout.md).

Rejestr terminali realizuje już lazy-start PTY po odtworzeniu modeli:
[terminal.md](terminal.md). Procesy są uruchamiane na nowo; snapshot nie
przywraca żywych procesów ani bufora terminala z poprzedniego uruchomienia.

## Weryfikacja

56 testów, fmt, Clippy i release. Testy obejmują dokładny roundtrip z metadanymi,
stabilne ID po restore, plan aktywacji, błędne dane, restart workera, nowszy
schemat oraz rollback wspólnej transakcji session/projekty.

W release: 2 taby, drzewo 3 pane'ów w pierwszym, aktywny pierwszy tab i oba
zamknięte sidebary. Natychmiastowe ⌘Q po zmianie zapisuje końcowy stan.
Po ponownym uruchomieniu i zamknięciu JSON całej sesji pozostał identyczny.
Nie wykonywano pomiarów FPS ani testów uruchamiania PTY.
