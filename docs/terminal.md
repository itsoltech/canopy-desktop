# Terminal i PTY

Alacritty Terminal 0.26.0 odpowiada za emulację, parser ANSI, bufor oraz PTY.
GPUI rysuje widoczne komórki; aplikacja nie osadza okna Alacritty.

## Uruchomienie

Pane ma LaunchSpec: executable, argv i cwd. Shell uruchamia shell użytkownika
jako proces PTY, a inne tool ID rozwiązujemy przez PATH odczytany z jego shella.
Nie wpisujemy komendy do działającej powłoki. Argumenty są przekazywane osobno,
więc znaki typu ; lub $ nie są interpretowane jako składnia shella.

Środowisko eksportowane przez shell login/interactive jest odczytywane w tle,
przy starcie katalogu narzędzi oraz przy Refresh/zapisie konfiguracji. Przechowujemy je wyłącznie w pamięci. Probe ma limit
5 sekund i 1 MiB, ignoruje komunikaty startowe poza oznaczonym blokiem oraz
nie wypisuje wartości zmiennych. PATH, TERM=xterm-256color, COLORTERM=truecolor
i TERM_PROGRAM=Canopy trafiają do procesu. Przed uruchomieniem GPUI i wątków
usuwamy odziedziczone po launcherze NO_COLOR, CLICOLOR, CLICOLOR_FORCE i FORCE_COLOR.
To istotne przy uruchamianiu z narzędzi automatyzacji, które ustawiają NO_COLOR=1.
Sam probe również startuje bez tych flag. Ustawienia jawnie eksportowane przez
konfigurację shella są później zachowywane; nie wymuszamy FORCE_COLOR.
Nie obsługujemy aliasów ani funkcji
jako executable; przy błędzie lub braku programu pane pokazuje Failed/Restart.

Na Windows nowa instalacja wybiera kolejno `SHELL`, jeżeli wskazuje poprawny
absolutny executable, następnie `pwsh.exe`, Windows PowerShell i `ComSpec`.
PowerShell oraz cmd nie otrzymują argumentu `-l`. Bazowe środowisko pochodzi ze
środowiska procesu GUI; Canopy nie wykonuje profili PowerShell podczas discovery,
więc błędny profil użytkownika nie blokuje terminala. `Path` i `PATHEXT` są
obsługiwane bez względu na wielkość liter, puste lub względne wpisy PATH nie
powodują wyszukania programu w przypadkowym cwd. EXE/COM są uruchamiane
bezpośrednio, skrypty PowerShell i Node przez jawny interpreter, a CMD/BAT przez
`ComSpec`. Adapter CMD odrzuca argumenty z `%`, cudzysłowem, NUL lub nową linią,
zamiast dopuścić niekontrolowaną ekspansję.

## Cykl życia i persystencja

Rejestr app_state/terminals.rs jest mapą PaneId → TerminalView. Plan z
terminal/lifecycle.rs uruchamia wyłącznie pane'y aktywnego taba aktywnego
workspace'u. Przełączenie na inny tab uruchamia go raz. Już uruchomione
nieaktywne taby zachowują procesy; nowe pozostają uśpione. Drag/drop i zamiana
miejscami zachowują PaneId, więc nie tworzą nowego procesu.

Stan runtime nie jest zapisywany w SQLite. Snapshot przechowuje konfigurację
pane'a; po restarcie aplikacji powstają nowe procesy tylko dla aktywnego taba.
Profile są rozwiązywane przez [katalog narzędzi](tools.md). Claude i Codex
wznawiają konkretny zapisany UUID sesji; szczegóły opisuje [integracja agentów](agents.md).

Starting → Running → Exited(code/signal) albo Failed. Po zakończeniu parser
opróżnia dostępny końcowy output, a supervisor zwalnia PTY i zbiera proces.
Pane zachowuje ekran i oferuje Restart / Close. Restart tworzy nowy PTY w tym
samym pane; Close usuwa pane z modelu. Usunięcie działającego pane'a lub
projektu kończy jego procesy. Zamknięcie aplikacji czeka również na rozpoczęte
sprzątanie pane'ów usuniętych chwilę wcześniej.

Sprzątamy zarządzaną grupę procesów i foreground job należący do sesji PTY.
Na Windows każda sesja tworzy osobny Job Object z `KILL_ON_JOB_CLOSE`.
Kontrolowany patch Alacritty przekazuje ten job przez
`PROC_THREAD_ATTRIBUTE_JOB_LIST`, więc proces główny należy do niego atomowo już
w `CreateProcessW`. Stop najpierw kończy cały job, a supervisor powtarza cleanup,
sprawdza wynik i przez maksymalnie 5 sekund odczytuje `ActiveProcesses` przez
`QueryInformationJobObject`. Dopiero potwierdzone zero zwalnia pliki tymczasowe
i daje pomyślny wynik zakończenia. Błąd lub timeout daje stan Failed, zachowuje
katalog tymczasowy i blokuje usuwanie danego worktree oraz Quit do czasu udanej
ponownej weryfikacji. `Session` zachowuje współdzielony wynik cleanupu niezależnie
od statusu procesu. Wszystkie wejścia Stop dołączają do jednej operacji widoku;
po błędzie kolejna próba ponownie odpytuje ten sam Job Object.
Potwierdzony wynik ma generację sesji i jest usuwany przed Restart, dlatego nie
może potwierdzić Stop kolejnego procesu. Zamknięty pane pozostaje niewidocznym
właścicielem cleanupu wraz z `PaneId` i `WorkspaceId` aż do potwierdzonego
wyniku; odpowiedni workspace nadal pokazuje Stop i blokuje usuwanie.
Pierwsze usunięcie prywatnego katalogu używa sprawdzanego `TempDir::close()`.
Sharing violation zapisuje ścieżkę oraz błąd w sesji; retry usuwa katalog po
zwolnieniu blokującego uchwytu i dopiero wtedy potwierdza cleanup.
Proces, który świadomie odłączy się od sesji (daemon/setsid), nie należy już
do tej grupy. Maksymalnie 64 PTY mogą działać jednocześnie.

## Widok i wejście

- Widoczna siatka, kolory ANSI/truecolor, bold/italic/underline, cursor,
  szerokie i łączone znaki Unicode.
- Klawiatura xterm, Ctrl+C, strzałki, klawisze edycji, natywny handler tekstu
  i kompozycji IME. Option-generowane znaki narodowe nie są zamieniane na Meta.
- Zaznaczanie myszą, Cmd+C/Cmd+V na macOS albo Ctrl+Shift+C/V na Windows,
  bracketed paste oraz scrollback 10 000 linii.
- Windows pozostawia niezmodyfikowane Ctrl+C/D/P/S/W/Z procesowi PTY. AltGr
  (Ctrl+Alt), polskie znaki i dead keys trafiają do natywnego handlera tekstu,
  gdy backend ustawia `prefer_character_input` i dostarcza `key_char`. Zwykłe
  Ctrl+Alt bez tekstu oraz Alt+B/Alt+F pozostają skrótami terminalowymi.
- Drop plików z platformy na działający terminal wstawia lokalne absolutne
  ścieżki zgodnie z aktywnym dialektem POSIX/PowerShell/CMD, w kolejności
  otrzymanej od systemu, z jedną końcową spacją i bez wysyłania Enter. CMD zawsze
  obejmuje ścieżkę cudzysłowami, a `%` i `!` odrzuca, ponieważ nie da się ich
  bezpiecznie traktować literalnie przy nieznanym stanie ekspansji. W trybie bracketed
  paste cała paczka trafia do jednej ramki. Drop akceptuje tylko żywą sesję
  Running pod kursorem; starting, stopping, zakończony proces i brak sesji nie
  dostają danych ani kolejki do przyszłego restartu.
- Obsługa trybu myszy aplikacji SGR/X10; Shift pozwala korzystać z zaznaczania
  lub scrollbacku zamiast raportowania myszy do aplikacji.
- Resize terminala i ioctl PTY na podstawie rzeczywistego rozmiaru pane'a.
- Bezpośrednio uruchamiane TUI otrzymują sygnały zmiany rozmiaru (SIGWINCH).
  Na Unix `terminal/spawn.rs` odblokowuje maskę sygnałów wyłącznie na czas
  utworzenia PTY/procesu, na bieżącym wątku. Następnie przywraca maskę workera,
  również po błędzie startu. To zapobiega dziedziczeniu przez Claude i inne TUI
  zablokowanych sygnałów z wątków dispatch macOS. Nie zmienia globalnych handlerów.

Alacritty respektuje synchronized output DEC private mode 2026 i publikuje
`Wakeup` dopiero po zakończeniu takiej aktualizacji lub jej timeoutu. Canopy nie
zamienia pomocniczych zdarzeń parsera w dodatkowe klatki. Codex i Claude nie
zawsze używają alternate screen ani 2026, dlatego stabilizacja wykrywa teraz
rzeczywisty redraw co najmniej ośmiu komórek w obu buforach. Komórki są
publikowane bez opóźnienia, a kursor zachowuje ostatnią stabilną, widoczną
pozycję do 80 ms ciszy po dużej serii zmian. Nie jest cyklicznie ukrywany przez
sam redraw. Kolejne fragmenty przedłużają wyłącznie ten aktywny burst. Zmiana
jednej komórki podczas zwykłego wpisywania publikuje pozycję natychmiast. Każde
zaakceptowane wejście użytkownika anuluje pozostały nieinteraktywny deadline;
duży redraw wywołany klawiszem jest scalany tylko przez 20 ms. Resize również
omija stabilizację, a jawne hide działa od razu. Timer nie istnieje w
spoczynku. Deterministyczna regresja dzieli primary-screen CSI pozycjonujące
kursor między odczyty parsera, wykonuje clear/redraw i sprawdza brak publikacji
pozycji pośrednich ani zmiany widoczności. Wizualne potwierdzenie Codex/Claude nadal wymaga natywnego
testu GUI.
- Jedna scalana notyfikacja outputu; widoczne aktualizacje są grupowane co
  8 ms. Nieaktywne widoki nie odrysowują się przy każdym kawałku outputu.
- Render nie czyta deskryptorów. Snapshot siatki używa try-lock, bez czekania
  na parser. Wejściowa paczka jest ograniczona do 1 MiB.

Drop plików nie kanonizuje ścieżek, nie czyta obrazów, nie zmienia schowka,
nie tworzy taba ani ImagePreview i nie zapisuje nic do SQLite. Ścieżki względne,
nie-UTF-8, ze znakami sterującymi lub paczka przekraczająca limit wejścia PTY są
odrzucane w całości z lokalnym komunikatem bez wypisywania pełnych ścieżek.
Finder przekazuje gotowe ścieżki przez mechanizm `ExternalPaths` GPUI. Payloady
typu file promise, URL, obraz ze schowka oraz wewnętrzny drag z Files pozostają
poza zakresem tej wersji. Rozpoznanie wklejonej ścieżki jako załącznika obrazu
należy do CLI Codex/Claude i wymaga osobnej weryfikacji w działającej sesji.

Font bazowy to JetBrains Mono, z jawnym systemowym fallbackiem do Nerd Fonts,
preferującym JetBrainsMono Nerd Font Mono / NFM. Glify Nerd wymagają fontu
zainstalowanego w systemie, podobnie jak w dotychczasowej konfiguracji Electrona.
Nie zmieniamy ustawień ani konfiguracji powłoki użytkownika.

## Geometria i tło

Siatka ma symetryczne marginesy poziome i pionowe. Prostokąty tła skrajnych
komórek są przedłużane do granic pane'a, łącznie z narożnikami. Nie dodajemy
komórek do emulatora, nie zwiększamy liczby kolumn/wierszy i nie kopiujemy glifów
na marginesy. Sąsiadujące tła o identycznym kolorze tworzą wspólny prostokąt;
dla jednolitego wiersza wystarcza jeden draw tła.

Podczas resize początek siatki pozostaje zakotwiczony, co usuwa skoki wynikające
z reszty dzielenia szerokości/wysokości przez rozmiar komórki. Po ustaniu zmian
przez duration::QUICK siatka jest centrowana jednokrotnie, bez interpolacji pozycji.
Jest najwyżej jedno zadanie ustalające geometrię, bez timera w spoczynku.
Mysz, zaznaczanie, kursor i IME używają tego samego początku siatki.

## Sprawdzenia i granice

Testy: literalne argv bez shella, końcowy ANSI/Unicode output, exit code i sygnał,
rzeczywiste stty size po resize, sprzątanie procesu ignorującego SIGHUP i jego
dzieci, błędy startu, timeout środowiska, plan lazy-start bez duplikatów,
tożsamość po drag/drop, pokrycie tła, centrowanie i stabilna pozycja podczas resize.
`tests/terminal_resize.rs` uruchamia bez shella proces testowy z wątku blokującego
SIGWINCH/SIGTERM. Sprawdza reakcje na kolejne rozmiary bez inputu, zgodność
siatki z TTY oraz przywrócenie maski workera po udanym i nieudanym starcie.
Regresja przed poprawką nie otrzymywała pierwszego SIGWINCH; po poprawce
przechodzi. W izolowanym GUI release na macOS sprawdzono Claude Code 2.1.267:
toggle obu sidebarów oraz zoom/przywrócenie okna aktualizują jego układ bez inputu.
Osobny proces diagnostyczny potwierdził pustą maskę dziecka i końcowy rozmiar
po animacji. Poprawka wymaga nowego procesu terminala; nie zmienia maski już
działających agentów.

W release sprawdzono powłokę fish z konfiguracją użytkownika, wejście poleceń,
Nerd Fonts, exit 7 oraz Restart. Użytkownik potwierdził poprawne wpisywanie.
Restore dwóch tabów uruchomił jeden proces; pierwsza aktywacja drugiego
uruchomiła drugi, zachowując PID już działającego. Zamknięcie aplikacji usunęło
oba procesy.

Zakres kwalifikacji: macOS. Nie wykonywano pełnego zestawu vttest, testów
czytników ekranowych, wszystkich metod IME ani pomiaru fizycznych 120 FPS.
Zaawansowany protokół klawiatury Kitty, OSC52 clipboard i wznawianie sesji agentów nie są włączone w tej wersji.
Session inspector korzysta z [integracji agentów](agents.md); wskaźnik zasobów pozostaje mockiem.
Git Changes i pane diffu są opisane w [git-changes.md](git-changes.md). Liczniki
narzędzi korzystają z rejestru działających procesów.

### Dokładne pozycje glifów

Renderer nie korzysta już z tolerancji force_width GPUI (która pozostawiała
odchylenia do 1 px). Każdy bajt kształtowanego fragmentu jest mapowany na kolumnę
komórki Alacritty, a pozycje glifów trafiają dokładnie na tę siatkę.
Zachowujemy wewnętrzne offsety znaków łączonych i fallbacków; tekst terminala
ma wyłączone ligatury między komórkami. Własny layout nie modyfikuje cache GPUI.

Regresja obejmuje ramki przy różnej długości kolorowanych fragmentów i kolumnach
1–199, mapowanie UTF-8, znaki łączone oraz szerokie komórki. W release sprawdzono
ramkę z 12 różnie kolorowanymi wierszami: prawa krawędź pozostaje wyrównana.

### Clear i scrollback

Systemowy `/usr/bin/clear` na macOS dla xterm-256color wysyła E3, home, ED2.
Alacritty 0.26.0 po E3 usuwa historię, ale ED2 przenosi poprzedni viewport
z powrotem do scrollbacku. Unixowy adapter odczytu PTY rozpoznaje dokładnie tę
sekwencję i dodaje końcowe E3. Nie rozpoznaje komend po wpisywanym tekście,
nie zmienia konfiguracji shella ani ogólnej semantyki ED2.

Stan rozpoznawania obejmuje granice odczytów; bajty są przekazywane bez czekania
na dalszy output. Dla zwykłego tekstu nie ma dodatkowych alokacji. Rejestracja
pollera, zapis, resize i sygnał zakończenia pozostają obsługą oryginalnego PTY.

Regresje w prawdziwym PTY: macOS clear i jego sekwencja po 80 wierszach dają
scroll offset 0; poprzednia treść nie wraca. Zwykłe home+ED2 zachowuje historię,
a czyszczenie alternate screen nie usuwa historii primary screen.

Tab, Shift+Tab i Shift+Enter mają akcje w kontekście Terminal, rejestrowane raz
po init GPUI. Dzięki temu globalna nawigacja Root nie przechwytuje ich przed
on_key_down. Przy fokusie powierzchni terminala wysyłamy wejście przez wspólny
encoder klawiszy: legacy xterm dla strzałek z modyfikatorami, C0/ESC dla
Ctrl/Alt+Backspace i CSI 13;2u dla Shift+Enter. Nie deklarujemy negocjowanego
protokołu klawiatury Kitty, dopóki encoder nie implementuje pełnej semantyki
tego trybu. Przy fokusie kontrolek procesu zachowujemy focus_next/focus_prev.
Autouzupełnienie unikalnej nazwy pliku w fish zostało sprawdzone w izolowanym GUI
release.

Resume oraz konfigurację hooków opisuje [integracja agentów](agents.md).
