> Terminale i rzeczywisty cykl życia procesów: [terminal.md](terminal.md).
> Aktualny kontrakt pełnego zapisu i odtwarzania: [persistence.md](persistence.md).

# Wspólny stan aplikacji

## Podział odpowiedzialności

`src/app_state.rs` rejestruje globalny AppState: zbiór uchwytów do osobnych
encji GPUI, a nie jeden obiekt odrysowujący wszystkie widoki.

| Moduł | Właściciel danych |
| --- | --- |
| SettingsState | zatwierdzone preferencje, status ładowania/zapisu, błędy i worker SQLite |
| ProjectsState + Projects | katalogi projektów, aktywny projekt, operacje pickera i cache workspace’ów |
| Workspace | taby, aktywny tab, drzewa splitów, fokus pane'a |
| LayoutState | szerokości i widoczność sidebarów, zakładka inspektora |
| NotchStatus | trwały błąd natywnego adaptera notcha i potwierdzone recovery |

Logika workspace'u w `src/state/workspace.rs` nie zależy od GPUI ani SQLite.
Widoki tłumaczą gesty na operacje domenowe, po udanej zmianie wywołując notify.
Animacje, hover, zaznaczenia kontrolek i uchwyty okien pozostają w widokach.
Subskrypcje mają jawnego właściciela.

Aplikacja startuje bez fixture projektu. Natywny picker otwiera prawdziwe katalogi;
lista otwartych projektów i wybór aktywnego są zapisywane w SQLite. Każdy projekt
ma własny workspace w pamięci. Procesy i odczyt drzewa plików nie są jeszcze podłączone.

## Taby i pane'y

Tab ma własne drzewo splitów. Liść zawiera PaneId i identyfikator narzędzia;
węzeł zawiera SplitId, oś, proporcję i dwójkę dzieci. Aktywny tab oraz
focused PaneId są osobnymi pojęciami. To zachowanie znane z Electrona,
bez przenoszenia jego zależności od procesów, IPC i rendererowych store'ów.

Identyfikatory są typowane i stabilne podczas życia procesu oraz wszystkich
operacji przenoszenia. Są również serializowane w pełnym snapshocie; odczyt rezerwuje je w generatorze.

Operacje modelu:

- open/activate/close — zarządzanie tabami i deterministyczny fallback wyboru;
- split/close_pane — podział i upraszczanie drzewa; ostatni pane zamyka tab;
- focus/set_ratio — aktywny pane i proporcje w zakresie 0.1–0.9;
- dock_pane/swap_panes — przenoszenie i zamiana pane'ów wewnątrz taba lub między tabami;
- detach_pane — pane do nowego taba, bez zmiany PaneId;
- dock_tab — całe drzewo taba do drugiego taba;
- reorder_tab/move_tab — kolejność tabów i przeniesienie między modelami workspace.

Dockowanie najpierw modyfikuje kandydacki model i waliduje wynik. Odrzucenie
celu lub przekroczenie maksymalnej głębokości 4 nie usuwa danych źródłowych.
Niepoprawne i niefinitywne proporcje są odrzucane. Przenoszenie pane'a nie
oznacza tworzenia nowej sesji; obecny liść nadal zawiera tylko dane mockowe.

## Interakcje

- Uchwyt w nagłówku pane'a rozpoczyna przeciąganie.
- Drop na krawędź pane'a tworzy podział; środek zamienia dwa pane'y miejscami.
- Drop taba na pane przenosi całe drzewo źródłowe. W środku celu wstawia je po prawej.
- Drop pane'a na wolne miejsce w pasku tabów tworzy nowy tab.
- Drop na istniejący tab ustawia nowy/przenoszony tab przed nim.
- Separator między pane'ami zmienia proporcję; krzyżyk zamyka pane.

| Skrót | Operacja |
| --- | --- |
| ⌘T | nowy tab z domyślnym narzędziem |
| ⌘W | zamknięcie aktywnego taba |
| ⌘D / ⌘⇧D | split poziomy / pionowy |
| ⌘⌥W | zamknięcie aktywnego pane'a |
| ⌘1 / ⌘2 | wybór pierwszego / drugiego taba |

Wszystkie te interakcje działają obecnie na mockach zawartości sesji.
Brak jeszcze przeciągania między oknami i zarządzania procesami; layout jest trwały.

## Ustawienia

Aplikacja używa jednego katalogu danych: jawnego `CANOPY_DATA_DIR`, a bez niego
`~/Library/Application Support/Canopy Rust` na macOS lub Known Folder
`LocalAppData/Canopy Rust` na Windows. Jeśli baza nie istnieje, tworzy własny
`canopy.db`. Nie wraca automatycznie do oryginału Electrona i nie nadpisuje
błędnej bazy. Task context, preview i prywatne profile korzystają z tej samej
decyzji katalogowej; user home jest rozstrzygany osobno. Rozwiązanie Known
Folder, utworzenie katalogu i ustawienie jego ochrony wykonują się na executorze
tła przed uruchomieniem workera SQLite.

Kontrolki General odczytują i zapisują reopenLastWorkspace, perf.hud.enabled,
newTab.toolId oraz newWorktree.toolId. Nazwy narzędzi są mapowane na identyfikatory;
obce ID pozostają opcjami Custom. Pozostałe widoki Preferences nadal są mockami.
notch.enabled jest w kontrakcie stanu, lecz nie steruje jeszcze oknem notcha.

Zapis jest pojedynczą operacją w toku. Kontrolki są wyłączone podczas ładowania
i zapisu; zatwierdzony snapshot jest publikowany dopiero po sukcesie SQL.
Błąd zachowuje poprzednią wartość i jest widoczny w Preferences.
Ustawienie widoczności wskaźnika zasobów odświeża główne okno; same metryki
CPU/RAM nadal są mockiem. Ustawienie domyślnego narzędzia steruje nowymi tabami.
Preferencja startup steruje odtwarzaniem projektów i pełnych drzew pane’ów.
Worktree obsługuje natywny moduł opisany w [git-worktrees.md](git-worktrees.md).

⌘Q i zamknięcie ostatniego okna czekają asynchronicznie na trwającą operację
i zamknięcie workera. Systemowe zakończenie procesu ma dodatkowy best-effort
hook GPUI (limit frameworka 200 ms); nie daje gwarancji przy wymuszonym ubiciu.

## Weryfikacja

- Formatowanie, Clippy z -D warnings i build release.
- 42 testy: w tym 9 operacji workspace i 9 zgodności SQLite.
- W release sprawdzono zagnieżdżone splity, przełączanie tabów, zamykanie pane'a,
  zapis checkboxa do SQLite i odczyt ustawienia po restarcie.
- Użytkownik potwierdził działanie interakcji po dodaniu uchwytów i drag/drop.
  Automatyczna próba przeciągania przez Computer Use została przerwana przez
  zmianę stanu aplikacji; nie traktujemy jej jako dowodu automatycznego testu.
- Nie wykonywano nowych pomiarów FPS.

Następnie: wersjonowany snapshot workspace/layout, zapis poza wątkiem UI
i odtworzenie drzewa oraz stabilnych ID po restarcie.


## Otwieranie projektów i pusty stan

- Bez aktywnego projektu widoczne są belka macOS i centralny Open folder.
  Sidebary, taby i status bar są odmontowane. Notch jest przezroczysty i ma pusty
  obszar przechwytywania myszy, dopóki nie ma aktywnego projektu.
- Open folder, ⌘O i + attach otwierają natywny picker pojedynczego katalogu.
  Anulowanie niczego nie zapisuje, a focus wraca do głównego okna.
- Kanonizacja ścieżki i sprawdzenie dostępu do katalogu wykonują się w tle.
  Nie wymagamy .git. Alias/symlink tego samego katalogu aktywuje istniejący wpis.
- Nowy projekt lub worktree ma pustą listę tabów. Utworzenie procesu wymaga
  jawnego wyboru narzędzia lub nowego taba.
- Kliknięcie projektu przełącza jego workspace. Cache w pamięci zachowuje taby,
  splity i fokus. Zamknięcie projektu usuwa go z listy i cache; nie usuwa folderu.
- Krzyżyk projektu albo ⌘⇧W zamyka projekt. Ostatni zamknięty projekt przywraca
  pusty stan. ⌘W nadal zamyka tylko tab.
- Nazwa i ścieżka wybranego folderu zasilają belkę, sidebar, mock terminala,
  inspektor i notch. Drzewo plików jest na razie puste, zamiast udawać pliki
  z referencyjnego projektu.

Operacje wyboru są serializowane; zmiana trafia do pamięci, a wspólny writer
zapisuje pełny snapshot. Błąd zachowuje ostatni poprawny zapis na dysku
i pokazuje komunikat o niezapisanym layoucie.
Przy zamykaniu aplikacji czekamy na trwający zapis; oczekiwanie na picker
może zostać anulowane przed rozpoczęciem zapisu.

SQLite: własna tabela _canopy_rust_projects, singleton id=1, version=1 i JSON
z listą kanonicznych ścieżek oraz aktywną ścieżką. Limit 128 projektów / 1 MiB.
Odczyt nie tworzy tabeli, zapis jest transakcyjny. Nie zmieniamy tabel workspace
ani migracji Electrona. Nowsza wersja lub uszkodzony zapis jest odrzucany.
To format Rust; nie jest importerem projektów Electrona.

Przy reopenLastWorkspace=true odtwarzamy dostępne foldery i wybór aktywnego.
Niedostępne foldery pomijamy z komunikatem, bez automatycznego usuwania ich
z zapisu. False daje pusty ekran i nie kasuje zapisanej listy. Kolejne jawne
otwarcie folderu zapisuje bieżącą listę. Taby/splity są odtwarzane z pełnej sesji. Początkowy mock pane’a jest tworzony
tylko dla nowego projektu lub przy odczycie starszego zapisu samej listy.

Motion: wejście pustego stanu, workspace’u i zmiana aktywnego projektu używają
Presence/CONTENT_REVEAL oraz distance::BASE. Belka pozostaje nieruchoma.
Reduce Motion kończy przejście natychmiast. Nie ma stałego timera ani odrysowań
po zakończeniu animacji; zmiana busy/error nie restartuje animacji stanu.

Weryfikacja tego etapu: 49 testów, fmt, Clippy i release. GUI: pusty ekran,
picker, anulowanie i powrót focusu, otwarcie rzeczywistego katalogu, dwa projekty
z zachowaniem splitów, zamknięcie ostatniego projektu, restart z odtwarzaniem
włączonym i wyłączonym. Preferencję testową przywrócono; aplikację pozostawiono
na pustym ekranie. Nie wykonywano nowych pomiarów FPS.
