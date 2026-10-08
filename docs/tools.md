# Tools i profile

Preferences → Tools edytuje katalog narzędzi. Claude/Codex w nawigacji otwierają
formularze opisane w [agent-preferences.md](agent-preferences.md). Własne narzędzie ma stabilny UUID, nazwę, executable,
argumenty i opcjonalne profile. Shell, Claude i Codex można wyłączyć; ich
identyfikatory pozostają stałe. Pozostałe presety można edytować lub usuwać.

Profil ma własny UUID, nazwę, model (Claude/Codex) i argumenty. W ramach narzędzia
wybieramy profil domyślny. Edytor przechowuje roboczą kopię; Save changes zapisuje
całą definicję wraz z profilami w jednej transakcji. Revert odrzuca draft.
Zmiana narzędzia w edytorze również porzuca niezapisany draft.

Sidebar rozwija profile wspólnym Disclosure/motion. Dla wielu profili strzałka,
ikona i nazwa są jednym nagłówkiem: kliknięcie rozwija/zamyka listę, a hover
rozjaśnia nazwę i strzałkę bez tła. Kliknięcie profilu uruchamia dokładnie ten
profil. Przy jednym profilu nagłówek uruchamia go bezpośrednio; przy braku profili
uruchamia bazową definicję narzędzia. Nie ma pustego miejsca po ukrytej strzałce.
Nowy tab i split otrzymują narzędzie z General. Liczniki pokazują rzeczywiście
uruchomione procesy, także w nieaktywnych tabach/projektach. Brak executable jest
sygnalizowany; błąd uruchomienia pozostawia pane z Restart/Close.

## Uruchomienie

Kolejność argv: argumenty narzędzia → `--model` profilu → argumenty profilu →
argumenty zapisane w pane. SQLite przechowuje gotową tablicę argv. Na Unix pola
edytują ją przez `shell-words`; na Windows używają odwracalnych reguł command-line,
które zachowują także argumenty złożone wyłącznie z backslashy, spacje,
cudzysłowy i puste argumenty. Żaden wariant
nie wykonuje interpolacji, potoków ani przekierowań. To bezpośredni proces PTY.
Środowisko pochodzi z istniejącej konfiguracji shella i logowania CLI użytkownika.
Klucze API profili korzystają z macOS Keychain. Nie importujemy sekretów safeStorage.

Na Windows domyślny Shell nie zapisuje uniksowego `-l`. Przy odczycie starszego
domyślnego wpisu dokładny, niezmodyfikowany wariant z samym `-l` jest migrowany;
własne argv użytkownika pozostaje bez zmian. Resolver obsługuje `Path`/`PATHEXT`
bez względu na wielkość liter i rozróżnia natywne EXE/COM od CMD/BAT, PowerShell
oraz skryptów Node. Każdy skrypt dostaje jawny interpreter i osobne argumenty.
Profilowe zmienne środowiska zastępują odziedziczone klucze bez względu na ich
wielkość liter; do procesu trafia jeden deterministyczny wpis dla każdego klucza.

ToolsState posiada katalog zatwierdzony przez SQLite oraz pojedynczy probe
środowiska. Refresh i zapis uruchamiają ponowne sprawdzenie executable w tle;
równoległe żądania są scalane. Render nie wykonuje I/O. Terminals pozostaje
właścicielem procesów. Zmiana konfiguracji nie restartuje działających procesów.
Restart i nowe uruchomienie odczytują aktualną definicję danego profilu.

## Persist / restore

Tabela `_canopy_rust_tools`, singleton `id=1`, `version=1`, payload JSON maks.
2 MiB. Nie zmieniamy tabel Electrona. Brak tabeli daje domyślny katalog w pamięci.
Niepoprawny lub nowszy payload jest odrzucany, bez automatycznego nadpisania.

Pane zapisuje tool ID i wybrany profile ID; zmiana profilu domyślnego nie
przepina istniejących pane'ów. Odtwarzanie pozostaje lazy: startuje aktywny tab,
kolejne przy pierwszym otwarciu. Usunięty profil lub wyłączone/usunięte narzędzie
dają jawny błąd, bez zamiany na Shell lub inny profil. Dane profilu są referencją
do aktualnej konfiguracji, nie historyczną kopią argumentów. `resume_id` pozostaje
nieobsługiwane i jest jawnie odrzucane.

Testy obejmują walidację/atomowość edycji, zachowanie profile ID po zmianie
domyślnego i serializacji layoutu, odrzucanie nieaktualnych referencji, literalne
argv w prawdziwym PTY, kod zakończenia, SQLite restart/read-only/nowszą wersję.

W release sprawdzono przez Preferences utworzenie własnego executable
`/usr/bin/printf` z profilem, zapis do SQLite, ponowny start aplikacji,
rozwinięcie sidebara i uruchomienie dokładnie tego profilu. Output zachował
literalne `$(echo literal)` i proces zakończył się kodem 0. Restart/Close
przetestowano; tymczasową definicję i pane usunięto po teście.

W sesji automatyzacji obraz okna odświeżał się po resize, mimo że akcje zmieniały
stan i zapisywały dane. Ocena płynności animacji na aktywnym ekranie pozostaje
niepotwierdzona; sam test funkcjonalny nie stanowi takiej kwalifikacji.
