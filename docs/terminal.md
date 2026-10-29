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
nie wypisuje wartości zmiennych. Na Unix probe tworzy nową sesję (`setsid`),
odłączoną od terminala uruchamiającego aplikację. Sama grupa procesów w tle
pozostawiała controlling TTY i zatrzymywała interaktywny bash sygnałem SIGTTIN.
Odczyt kończy się po pełnym znaczniku środowiska, bez czekania na exit hooks
lub background job trzymający stdout. W każdym wyniku kończymy grupę probe’a
i zbieramy jego proces. Rzeczywisty timeout wskazuje shell i akcję Refresh
w Preferences → Tools; nie zastępujemy błędu pozornie poprawnym środowiskiem.
PATH, TERM=xterm-256color, COLORTERM=truecolor
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
- Linki (`terminal/links.rs`, jak `addon-web-links` i `linkHandler` w Electronie):
  adresy http(s) w tekście, także przełamane przez zawinięcie linii, oraz
  hiperłącza OSC 8 wypisywane przez programy (Claude, Codex). Kropka, przecinek
  itp. na końcu i niesparowane nawiasy zamykające nie należą do adresu. Link pod
  kursorem jest podkreślony w swoim kolorze, a kursor zmienia się w dłoń. Kliknięcie
  bez przeciągnięcia (puszczenie na tej samej komórce) otwiera go w przeglądarce
  systemowej; przeciągnięcie zaznacza tekst. Otwierane są tylko adresy
  http/https, inne schematy OSC 8 (np. `file://`) są ignorowane. W trybie myszy
  aplikacji kliknięcie dostaje program, link działa z Shiftem. Brak wbudowanej
  przeglądarki i preferencji `urlOpenMode` z Electrona: zawsze przeglądarka systemowa.
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

Font bazowy to JetBrains Mono (zmienny w Settings → Appearance, zob. „Font
terminala”), z jawnym systemowym fallbackiem do Nerd Fonts, preferującym
JetBrainsMono Nerd Font Mono / NFM. Glify Nerd wymagają fontu
zainstalowanego w systemie, podobnie jak w dotychczasowej konfiguracji Electrona.
Nie zmieniamy ustawień ani konfiguracji powłoki użytkownika.

## Właściciel wejścia i remote

Każdy start procesu tworzy `terminal::control::Run` z nowym UUID runu (nigdy
ponownie użytym, także po Restart). Run ma jednego właściciela wejścia i rozmiaru:
`Holder::Local` (okno hosta) albo urządzenie. Każde przyznanie zwiększa fencing
token. Lokalne pisanie, wklejanie, drop plików, task prompt i resize przechodzą
przez te same sprawdzenia co RPC. Pisanie na hoście zajmuje wolną dzierżawę,
ale nigdy nie odbiera jej urządzeniu: pasek „Input is controlled by …” z akcją
„Take control” jest jawnym przejęciem. Gdy urządzenie trzyma wejście, siatka
hosta przyjmuje jego rozmiar. Dzierżawa urządzenia wygasa po 15 s bez odnowienia
(odnowienie co 5 s) albo gdy zamknie się jego ostatni strumień; proces trwa.
Powtórzony input ID tego samego właściciela i tokenu zwraca pierwszy wynik bez
ponownego zapisu. ACK oznacza „queued” na hoście, nie wykonanie przez shell.

Wejście PTY jest ograniczone: `terminal/metered.rs` zlicza bajty od przyjęcia
do faktycznego zapisu do PTY (1 MiB na sesję, 16 MiB łącznie). Odpowiedzi
emulatora (DA, kolory, rozmiar) są liczone, ale nie odrzucane. Pełna kolejka daje
lokalny komunikat albo `resource_limit` w ACK.

`remote/terminal_rpc.rs` implementuje TerminalService. Strumień TerminalSession
czyta tę samą projekcję Alacritty co okno hosta (`Session::live_frame_bounded`) i nie
zmienia lokalnego scrolla ani zaznaczenia. Pierwsza ramka jest pełna, kolejne
to łaty względem tego, co ma dany subskrybent. Ramka jest liczona dopiero, gdy
kolejka wyjściowa ma miejsce, więc wolny klient dostaje rzadsze, scalone łaty
i nie blokuje parsera. Wire używa stałych masek TerminalInputMode/TerminalCellFlag,
RGBA `R<<24|G<<16|B<<8|A`, `column_span` 2 dla szerokiego znaku i 0 dla spacera.
Kolory są końcowe: INVERSE i DIM są już w nich zastosowane i nie trafiają do flag.
Ramka powyżej 512 KiB idzie fragmentami ≤64 KiB (SHA-256, ciągłe offsety, bez
przeplatania, maks. 32 MiB). Każdy fragment ma sequence; klient zatwierdza kursor
po zweryfikowanej instalacji. Reconnect zawsze dostaje pełną projekcję. Restart
kończy strumień jawnym `aborted` z poleceniem ponownej subskrypcji.

ReadHistory zwraca start, ramkę (lub fragmenty) i end dla maksymalnie 200 wierszy
z `Session::history_page_bounded`. Token strony wiąże run, rewizję projekcji i offset
(MAC tego uruchomienia serwera); nowy output daje `aborted` i wymaga świeżego
odczytu. `lost_scrollback` oznacza wyczerpany limit 10 000 linii.

Terminal i czat korzystają z tego samego właściciela budżetu wire: 4 MiB na
urządzenie i 32 MiB łącznie. Terminal ma jeden element kolejki transportowej;
oczekujące ACK/błędy też rezerwują bajty. Limit 64 odczytów obejmuje zarówno
TerminalSession, jak i ReadHistory. Osobne 32 MiB na host ogranicza materiał
projekcji: skopiowane komórki i tekst, zachowany baseline `Installed`, kopie
Protobuf oraz kodowanie. Każdy klient mieści się również w tym globalnym limicie.
Estymacja i rezerwacja poprzedzają kopiowanie pod tą samą blokadą siatki; znaki
łączone są uwzględnione bez kopiowania ich tekstu. RAII przenosi opłatę baseline'u
i zwalnia tymczasowe kopie po kodowaniu. Jeżeli poprzedni baseline uniemożliwia
przyjęcie zamiennika, host zwalnia baseline i wysyła pełną ramkę zamiast łaty.
Fragmenty powstają leniwie z jednego kodowania, bez wektora wszystkich fragmentów.

Oczekiwanie na transport nie zatrzymuje input, resize, renew ani reakcji na zmianę
uprawnień. Odebranie TERMINAL przy zachowanym READ zwalnia dzierżawę wejścia,
ale pozostawia dozwolony odczyt. Pięć sekund bez postępu kończy odczyt
z `resource_exhausted` i powodem ponownego odczytu/subskrypcji. Wynik końcowy omija pełną kolejkę: częściowa ramka
nie kończy się pozornie poprawnym EOF. ReadHistory zwalnia workspace także wtedy,
gdy transport przestał odpytywać stream; autoryzacja jest sprawdzana przed każdą
porcją i co 500 ms podczas blokady. Oczekiwanie na zajętą siatkę ma limit 5 s.

Regresje komponentowe obejmują wspólny budżet czatu/terminala przez drop i revoke,
rezerwację dużych ramek i odzyskanie pamięci, input/resize przy nieczytającym
odbiorcy oraz timeout historii. Test obciążenia komponentów używa tymczasowej
bazy ze 100 000 elementów czatu, 8 strumieni syntetycznych po 100 delt/s
po 1 KiB przez 3 s, 8 urządzeń/32 subskrypcji, rezerwacji chunków bulk
i jednego nieczytającego urządzenia. Sprawdza
ciągłość u pozostałych, granicę paginacji i odzyskanie po resync. To nie jest
AC-30: LAN, natywny apply/render, 60 s rozgrzewki i 10 min pomiaru pozostają
NOT RUN, podobnie jak kwalifikacja GUI i macOS/Windows/iOS/Android tej zmiany.

Uprawnienia: odczyt i strumień wymagają READ dla kanonicznego roota terminala;
input, resize, przejęcie, Open, Stop i Restart wymagają TERMINAL. Strumień
co 5 s ponownie sprawdza certyfikat i grant, a zapis co najwyżej co 1 s. Open,
Stop i Restart są trwałymi komendami (accepted → attempting → wynik) wykonywanymi
przez zadanie, które przeżywa zerwane RPC. Stop i Restart wskazują run i czekają
na potwierdzony cleanup. Terminal otwarty przez urządzenie
(`Terminals::open_detached`) nie tworzy pane'a ani zmiany layoutu hosta. Należy
do workspace'u: liczy się do Stop/usuwania worktree i limitu 64, kończy się przy
zamknięciu projektu, `TerminalBridge::close` i Quit. Ma ID deterministyczne
dla komendy, więc ponowione Open nie uruchamia drugiego procesu. Terminale pane'ów
mają ID UUID nadawane widokowi na czas jego życia. Testy obejmują automat
dzierżawy, projekcję, fragmenty, tokeny historii i deduplikację komend oraz
izolowany proces `/bin/cat` w PTY dla backpressure, uprawnień i disconnect.
Nie uruchamiano GUI, realnych providerów ani magazynu sekretów.

### Widok terminala zdalnego (desktop)

`ui/remote_client/terminal.rs` składa stronę z tych samych komponentów co lokalny
pane: `tab_strip`/`workspace_tab` z terminalami bieżącego workspace'u z katalogu
hosta (kropka oznacza zakończony proces), `terminal_surface` + `terminal_viewport`
z canvasem wypełniającym dostępną wysokość i `process_status_bar`. Bez dzierżawy
pasek mówi „Input is controlled by another device…” (gdy katalog zna innego
właściciela) albo „Read only…”, z przyciskiem Take control. Historia ma pasek
„Showing earlier output…” z Back to live output, zakończony proces — Restart.
Akcje są ikonami z tooltipem w pasku zakładek: Earlier output, Copy selection
i menu Terminal actions (Release control, Restart terminal…, Stop terminal…,
Close view). Wszystkie wywołują te same operacje co wcześniej.

Malarz używa geometrii lokalnej (`GridGeometry` z komórką fontu terminala
urządzenia, zob. „Font terminala”, wyrównanie glifów do siatki, scalanie teł i rozciąganie skrajnych
teł do marginesów, zaznaczenie `selected`, kursor 45%/18% zależnie od fokusu).
Siatka ramki hosta jest centrowana jak lokalnie; mysz i zaznaczanie liczą
komórki z tej samej geometrii. Lokalnego `TerminalView` nie użyto ponownie:
jest związany z sesją PTY/Alacritty, kursorem i lifecycle'em pane'a, więc adapter
ramek zdalnych wymagałby osobnej abstrakcji. Wypełnienie wysokości zależy od
rodzica: kontener strony w `remote_client/mod.rs` jest przewijany, dlatego
powierzchnia ma minimalną wysokość (`min_h_96`) do czasu, aż strona Terminal
dostanie nieprzewijany kontener `size_full`.

## Paleta kolorów i motyw

Domyślne kolory terminala (ANSI 0–15, tło i pierwszy plan) należą do aktywnego
wariantu motywu: `canopy_desktop::appearance::TerminalColors` w
`src/appearance/palettes.rs`, po jednym zestawie na wariant (oficjalne palety
terminalowe rodzin, Canopy Dark zachowuje dawną paletę z tłem `#202020`).
Indeksy 16–255 (kostka xterm i rampa szarości) nie zależą od motywu; pozostałe
kolory nazwane używają pierwszego planu, `NamedColor::Background` — tła.
`session::palette(index)` czyta `appearance::active_terminal()` (jeden atomowy
odczyt, bezpieczny z dowolnego wątku), także dla odpowiedzi na OSC color request.
Kolory z sekwencji (truecolor) i nadpisania OSC zostają bez zmian.

Komórki `Frame` mają kolory rozwiązane w chwili projekcji, dlatego zmiana motywu
(`ui::theme::bind`) wywołuje `TerminalView::repaint_palette`, czyli
`Session::notify`: lokalny widok i obserwatorzy zdalni odczytują ramkę ponownie,
bez zmiany emulatora i rewizji. Widok zdalny maluje kolory RGBA wyliczone przez
hosta — pokazuje paletę motywu hosta, nie urządzenia (protokół nie przenosi
indeksów kolorów).

## Font terminala

`terminal.font_family` (lista po przecinku) i `terminal.font_size` (8–24 px,
domyślnie JetBrains Mono 13 px) ustawia Settings → Appearance. Właściciel motywu
(`ui::theme::bind`) przy każdej zmianie tych ustawień buduje `TerminalFont`
(global `TerminalFontState`, odczyt `theme::terminal_font(cx)`):

- rodzina główna to pierwsza zainstalowana z listy użytkownika, a gdy żadna nie
  jest — pierwsza zainstalowana z łańcucha Canopy (JetBrains Mono, JetBrainsMono
  Nerd Font Mono/NFM/Nerd Font/NF, Symbols Nerd Font Mono, FiraCode Nerd Font,
  Menlo). Pozostałe nazwy (także niezainstalowane z listy użytkownika) zostają
  fallbackami glifów w tej kolejności, bez powtórzeń. Listę zainstalowanych
  rodzin (`TextSystem::all_font_names`) czytamy raz, w tle; do tego czasu rodziną
  główną jest pierwsza z listy;
- komórka (`terminal::geometry::CellSize`) jest mierzona z fontu: szerokość to
  advance znaku `m` w rozwiązanej rodzinie, wysokość zachowuje dawny stosunek
  17/13 (zaokrąglony do pikseli; 13 px → 17 px). Bez pomiaru szerokość wynika ze
  stosunku domyślnej komórki 7,83/13.

`GridGeometry::fit/with_size` przyjmują `CellSize`. `TerminalView::resize`
(prepaint canvasu) porównuje komórkę z ostatnią; zmiana fontu działa jak resize:
nowa siatka, `local_resize` PTY (SIGWINCH dla TUI), ramka po reflow w tej samej
klatce i natychmiastowe wycentrowanie zamiast czekania na koniec resize. Malarz
kształtuje tekst rozmiarem fontu i wyrównuje glify do szerokości komórki; mysz,
zaznaczanie, kursor, IME i przewijanie kółkiem liczą w tej samej komórce. Widok
terminala zdalnego używa fontu urządzenia i z niego wylicza rozmiar siatki
proszony u hosta. Zmiana odświeża okna; widoki ukryte przeliczą siatkę przy
następnym pokazaniu. Widoki kodu (edytor, bloki kodu) zostają przy fontcie
`theme::MONO` motywu.

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

## Panel narzędzi worktree

Terminale mają osobny wybór taba od centralnej rozmowy. Plus tworzy kolejną
sesję, Split niezależny pane z odziedziczonym cwd i profilem. Przycisk przy
worktree przywraca istniejący terminal lub tworzy Shell, jeżeli żadnego nie ma.
Ukrycie, maksymalizacja i przeniesienie panelu na dół zachowują PTY i tożsamości.
Dopiero jawne zamknięcie uruchamia dotychczasowy cleanup. Lazy-start respektuje
widoczność panelu przy restore. Szczegóły: [układ](features/workspace-layout.md).
