# Git i worktree

Git2 0.21.0 z vendored libgit2 1.9.7 obsługuje lokalne repozytoria bez procesów
`git` i bez shella. Transporty sieciowe git2 nie są włączone. Watcher korzysta
z notify 7.0.0, już obecnego w grafie GPUI.

## Zakres

- Wykrycie repozytorium, głównego katalogu, HEAD, lokalnych branchy i linked worktree.
- Lista worktree pod repozytorium; istniejące katalogi można wybrać bez ponownego attach.
- Jeden formularz tworzy nowy branch z wybranego lokalnego brancha albo otwiera
  wolny istniejący branch. Canopy generuje stabilną propozycję katalogu
  `<repo>-<10 znaków UUID>` obok głównego repozytorium i ponawia generowanie,
  jeśli ścieżka została zajęta przed wykonaniem. Symlinki i istniejące ścieżki
  nie są nadpisywane. Branch zajęty w worktree można otworzyć z formularza.
- Każdy otwarty katalog ma stabilny WorkspaceId i własny layout tabów/pane'ów.
  `repository_path` grupuje je w sidebarze; `path` identyfikuje konkretny worktree.
- Nowy projekt/worktree jest pusty: bez tabów, pane'ów i automatycznego procesu.
  Użytkownik wybiera narzędzie z sidebara albo otwiera nowy tab. Zapisany układ,
  również pusty, wraca przy przełączeniu i po restarcie. Restore startuje procesy
  tylko dla istniejących pane'ów aktywnego taba. Pozostałe taby startują lazy.
- Usuwanie domyślnie zachowuje branch. Formularz pozwala też usunąć lokalny
  branch albo scalić go do wskazanego lokalnego brancha przed cleanupem.
  Analiza pokazuje commity nieosiągalne z konkretnego celu oraz wynik merge:
  already integrated, fast-forward, merge commit albo konflikty. Utrata
  niewłączonych commitów wymaga osobnego potwierdzenia. Branch jest usuwany
  dopiero po udanym usunięciu worktree; cleanup brakującego wpisu zawsze go zachowuje.
  Merge wykonuje wyłącznie commity, bez fetch/push/stash/rebase, i musi zakończyć
  się przed usuwaniem katalogu. Konflikt nie modyfikuje refów, indeksu ani plików.
  Zgoda na usunięcie plików jest związana z odciskiem konkretnych ścieżek,
  statusów, indeksu, zawartości i trybów plików; identyczna liczba zmienionych
  wpisów nie wystarcza do ponownego użycia zgody.
  Działające procesy Canopy
  wymagają osobnej zgody na zamknięcie; po niej aplikacja czeka na zakończenie PTY.
  Zmienione, untracked lub ignored pliki wymagają kolejnej, osobnej zgody na
  trwałe usunięcie. Anulowanie zachowuje pliki i layout; wcześniej zatrzymane
  procesy nie są automatycznie restartowane. Lock, detached HEAD, submoduły
  i niedokończone operacje Git nadal blokują usunięcie.
  Checkoutowany cel merge musi być czysty i bez działających procesów Canopy;
  aktualizacja obejmuje właściwy HEAD worktree, ref, indeks i working tree.
  Hooki przed commitem widzą przygotowany wynik merge, a ich zmiany indeksu
  wchodzą do podpisywanego commita. Przed ich uruchomieniem zapisujemy standardowy
  stan `ORIG_HEAD`/`MERGE_HEAD`/`MERGE_MSG`/`MERGE_MODE`; odrzucenie hooka lub
  podpisu pozostawia merge możliwy do dokończenia albo przerwania narzędziami Git.
  Błąd `post-merge` po publikacji jest
  ostrzeżeniem przy zachowanym wyniku merge. Nie wykrywamy procesów
  uruchomionych poza Canopy, które mogą używać katalogu.
- Dla brakującego katalogu X otwiera osobne potwierdzenie usunięcia nieaktualnego
  wpisu. Worker usuwa wyłącznie wskazaną rejestrację Git, bez flag kasowania
  working tree lub wymuszania usunięcia valid/locked worktree. Branch pozostaje.
  Dopiero sukces zamyka zapisany workspace; działające procesy i niezapisane
  bufory zachowują zabezpieczenia zwykłej ścieżki usuwania.
  Sprawdzamy ponownie nazwę rejestracji, ścieżkę, brak katalogu i locki.
  Istniejący, lecz uszkodzony/niedostępny katalog oraz dangling symlink nie są
  traktowane jak brak katalogu. Zmiana ścieżki rejestracji lub odtworzenie katalogu
  blokuje cleanup. Wpis już usunięty przez inne narzędzie można zapomnieć.
  Kanonizacja istniejącego rodzica zachowuje tożsamość workspace'u po zniknięciu
  katalogu, również dla aliasów typu /tmp → /private/tmp.
  Ostrzeżenia watcherów opisują bieżące awarie, nie ich historyczny licznik;
  po zamknięciu brakującego workspace'u jego ostrzeżenie znika i nie jest
  traktowane jako błąd zakończonej operacji Git.

## Koszt odczytów

`git/service.rs` posiada jeden worker i ograniczoną kolejkę 64 poleceń.
Repozytoria są odczytywane poza UI. Otwarte worktree tego samego repozytorium
współdzielą `Arc<RepositoryInfo>`; ich cold start wykonuje jeden pełny odczyt
metadanych. UI dostaje tylko zmienione snapshoty, z pojedynczym scalanym sygnałem.

Watchery obserwują wspólny gitdir bez rekursji oraz refs/worktrees rekurencyjnie.
Nie obserwują plików roboczych ani objects. Zdarzenia są scalane przez 200 ms;
strumień zdarzeń nie przesuwa deadline bez końca. Flagi dirty chronią przed
utratą informacji przy pełnej kolejce. Dodanie/usunięcie katalogu metadanych
odbudowuje odpowiednie watchery. Zamknięcie repozytorium zwalnia jego watchery.
Przed mutacją usuwającą worktree worker zwalnia także watcher Changes wybranego
katalogu, a AppState zatrzymuje Files oraz watchery edytorów i preview należących
do źródłowego i ewentualnego docelowego workspace'u. Błąd lub kolejny krok
potwierdzenia odtwarza je; po potwierdzonym usunięciu nie otwieramy uchwytów ponownie.

W spoczynku worker czeka na zdarzenie — bez cyklicznego skanowania. Nie wykonuje
status/diff na każdą zmianę pliku. Pełny status służy kontroli usuwania
oraz osobnemu, scalającemu zdarzenia watcherowi widocznego Git Changes. `Refresh Git metadata` w menu kontekstowym nagłówka projektu zapewnia ręczne odświeżenie;
błąd watchera jest widoczny w sidebarze. Git Changes/diffy opisuje [git-changes.md](git-changes.md).

Ograniczenia: 128 otwartych katalogów, 128 linked worktree na repozytorium,
4096 lokalnych branchy. Aktualizacje indeksu przy kontroli statusu są wyłączone.
Operacje modyfikujące Git są serializowane; podczas nich nowe uruchomienia
narzędzi są wstrzymane. Już działające procesy pozostają aktywne.

## Weryfikacja

Testy używają wyłącznie libgit2 i repozytoriów tymczasowych. Obejmują tworzenie,
wykrycie z linked worktree, branch zajęty, ochronę danych, locked/ignored,
zachowanie brancha po usunięciu, współdzielenie cache i persist/restore layoutu.

Pomiar regresyjny: 30 nowych branchy w serii → 1 odczyt metadanych. W spoczynku
oraz po 100 zmianach zwykłego pliku → 0 dodatkowych odczytów Git. To pomiar liczby
operacji, nie deklaracja określonego CPU/FPS dla dowolnego repozytorium.
W GUI sprawdzono listę istniejących worktree, branch w status barze i formularz
Create worktree. Tworzenie/usuwanie testowano na repozytoriach tymczasowych.

Potwierdzenia nie są rozpoznawane po treści błędu: worker zwraca typowany
`RemovalOutcome`/`WorktreeRemovalOutcome`, a modal przechodzi między typowanymi
krokami Processes/Changes/Branch. Zmiana OID lub statusu po analizie zwraca nową
analizę zamiast wykonywać operację. Częściowy wynik rozróżnia merge, cleanup i
usunięcie brancha, dzięki czemu ponowienie nie publikuje drugi raz udanego merge.
`RemoveWorktree` rozróżnia usuwanie katalogu i cleanup brakującej rejestracji;
zgoda na ten drugi nigdy nie przechodzi w usuwanie istniejących plików.
Po zamknięciu procesów stan plików jest sprawdzany ponownie. Kolejny krok jest
aktywny dopiero po animacji wejścia, aby szybki podwójny klik nie potwierdził
obu pytań. Testy sprawdzają brak mutacji przed zgodą, usunięcie changed/untracked/
ignored po zgodzie i zachowanie zabezpieczeń głównego oraz locked worktree.

Cleanup brakujących wpisów sprawdzono w 20 testach Git, w tym dla odtworzonego
katalogu, dangling symlink, locków, zmienionej rejestracji, aliasu rodzica
i odświeżenia cache/ostrzeżeń. W izolowanym GUI release otwarto testowy worktree,
usunięto jego katalog poza aplikacją i odtworzono zapisany workspace. Anulowanie
zachowało wpis; potwierdzenie usunęło wyłącznie jego rejestrację i zapisany layout,
zachowując branch i pozostałe worktree. Ostrzeżenie watchera zniknęło po cleanup.

## Stop procesów

Czerwony kwadrat po prawej stronie worktree jest widoczny, gdy działają jego
pane’y. Zatrzymuje procesy wszystkich tabów tego workspace’u (również
nieaktywnych), zachowując taby, layout i końcowy ekran terminala. Po zakończeniu
pozostają Restart/Close. Nie startuje uśpionych tabów. Zadanie zamykania jest
utrzymywane przez rejestr terminali; ponowny Stop i Restart są blokowane do jego
zakończenia. Usuwanie worktree oczekuje na już trwające zatrzymanie, zamiast
rozpoczynać drugi równoległy cleanup.

## Czytelna tożsamość worktree

Snapshot rozróżnia branch, unborn branch, detached commit i niedostępny HEAD.
`HEAD` zwracany przez shorthand nie jest nazwą brancha. Sidebar nadal pokazuje
nazwę brancha dla zwykłych worktree; detached/unavailable używają nazwy katalogu.
Jeśli katalog powtarza nazwę repo lub koliduje z innym wierszem, dodajemy najkrótszy
rozróżniający fragment rodzica, np. `pinpoint (bb23)` i `pinpoint (044b)`.
Tooltip pokazuje pełną ścieżkę, stan HEAD i skrót commita; status bar wskazuje
`Detached HEAD · <commit>`. Lock zachowuje dotychczasowe oznaczenie.

Nazwy są przygotowane na workerze jako prezentacja, oddzielnie od `WorktreeHead`.
Operacje i wybór brancha korzystają z `branch_name()`, a routing workspace'u z path.
Etykieta nie jest nową tożsamością ani podstawą resume, usuwania lub przełączania.
Zmiana nie modyfikuje branchy, HEAD, plików worktree ani zapisanych layoutów.

Weryfikacja etykiet: 13 testów `tests/git.rs` przeszło sekwencyjnie, w tym detached
worktree z jednakowym basename, niedostępny katalog i unborn HEAD. W release GUI
sprawdzono `workspace (044b)` / `workspace (bb23)`, wybór właściwej ścieżki,
tooltip z pełną ścieżką i commitem oraz dolny status `Detached HEAD · <hash>`.
Test używał osobnej bazy i disposable worktree; istniejących danych użytkownika
ani realnych branchy nie zmieniano. Clippy, fmt i release build przeszły.
