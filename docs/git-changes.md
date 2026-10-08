# Git Changes, diff i commit

Changes w prawym panelu korzysta z libgit2. Nie ma subprocessów `git` ani
wywołań shella do statusu, stage, unstage, discard czy tworzenia obiektów/refów.

## Przepływ

- Oddzielne listy staged/unstaged, filtr ścieżki i statusów A/M/D/R; typechange
  oraz konflikty mają własne oznaczenia. Plik częściowo staged występuje w obu listach.
- Stage/unstage pojedynczego pliku lub wszystkich aktualnych zmian.
- Discard dotyczy working tree, zachowuje staged zawartość i wymaga modala.
  Untracked plik jest usuwany. Ścieżki są literalne, bez rozwijania globów;
  parent traversal i wyjście przez symlink katalogu są odrzucane.
- Kliknięcie pliku otwiera diff w pane. Porównanie staged to HEAD↔index,
  unstaged to index↔working tree. Nazwa pliku, strona porównania i cwd zapisują
  się w PaneMetadata; odtworzenie diffu nie uruchamia PTY.
- Istniejący pane danego porównania jest aktywowany zamiast duplikowania.
  Przenoszenie/splitowanie korzysta ze zwykłego modelu pane'ów.
- Commit obejmuje wyłącznie indeks. Wiadomość jest zachowywana przy błędzie
  i oddzielnie w pamięci dla worktree. Sukces czyści wiadomość danego worktree.
- Opóźnione akcje z nieaktualnego worktree nie stage'ują danych w nowym kontekście.

## Wydajność i aktualizacja

Jeden worker Git serializuje operacje. Changes ma osobny watcher tylko dla
wybranego worktree, gdy Changes lub diff są widoczne. Zamknięcie widoków zwalnia
watcher; nie ma timera odczytującego status w spoczynku.

Callback filesystemu zapisuje maksymalnie 256 kandydatów i sygnał dirty.
Zdarzenia są scalane przez 300 ms. Worker sprawdza ignorowanie ścieżek przed
statusem; przepełnienie listy powoduje bezpieczny pełny refresh. Zmiany indeksu,
HEAD i własne operacje również odświeżają snapshot. Revision umożliwia aktualizację
contentu diffu także wtedy, gdy nazwy/statusy plików się nie zmieniły.

Listy plików i linii diffu są wirtualizowane. UI nie tworzy przycisków/wierszy
dla całego wielkiego diffu. Preview ma limity 2 MiB / 20 000 linii oraz limit
pojedynczej linii; przekroczenie jest jawnie oznaczone. Binary nie udaje tekstu.
Jeden pane ma najwyżej jedno pobranie diffu w toku i scalone żądanie kolejnego.

## Podpisywanie

Konfiguracja: commit.gpgSign, gpg.format, user.signingKey, gpg.program,
gpg.openpgp.program i gpg.ssh.program. Canopy nie zmienia tych ustawień.

- OpenPGP: bezpośrednio GPG, armored detached signature, normalny gpg-agent
  i pinentry. Produkcyjny kod nie pobiera hasła, nie ustawia loopback pinentry
  ani nie przekazuje passphrase w argv. Graphical pinentry wymaga poprawnej
  instalacji/konfiguracji użytkownika, np. pinentry-mac na macOS.
- SSH: ssh-keygen lub skonfigurowany signer, namespace `git`; obsługiwane są
  ścieżki kluczy oraz `key::`/literalny public key z ssh-agent. SSH_AUTH_SOCK
  pochodzi ze środowiska shella. Skonfigurowany SSH_ASKPASS jest obsługiwany;
  bez agenta/askpass zaszyfrowany klucz może wymagać wcześniejszego odblokowania.
- Wywoływany jest program podpisujący, nie `git commit`. Nie czytamy prywatnego
  klucza ani hasła w kodzie aplikacji; otrzymujemy gotowy podpis.
- Timeout 120 s, ograniczone wyjście signera, Cancel commit. Błąd, anulowanie,
  brak klucza/programu lub nieobsługiwany format nigdy nie daje unsigned fallback.
- Podpis armored musi zawierać prawidłowy marker BEGIN i END. Timeout końcowego
  drainu stdout/stderr jest błędem, nawet gdy signer zakończył się kodem zero.
- Windows wymaga natywnego programu EXE/COM. Signer startuje atomowo w Job Object,
  bez pomocniczego okna konsoli; potomkowie i pipe'y są domykane przed wynikiem.
  Graficzny pinentry/SSH_ASKPASS zachowuje możliwość pokazania własnego okna.

Libgit2 przygotowuje commit buffer przed podpisem. Po podpisaniu ponownie
sprawdzamy HEAD i indeks; blokady HEAD/ref/index chronią publikację przed
równoległą operacją Git. Zmiana indeksu/HEAD w trakcie podpisywania przerywa
commit. Sam obiekt jest tworzony przez libgit2, następnie aktualizowany jest
branch i reflog. Blokad indeksu/refów nie trzymamy podczas oczekiwania na hasło.

Dokumentacja providerów: [GnuPG options](https://www.gnupg.org/documentation/manuals/gnupg/GPG-Configuration-Options.html),
[ssh-keygen signing](https://man.openbsd.org/ssh-keygen).

## Granice etapu

Pull/Push opisuje [transport Git](git-network.md). Nie ma stage hunków,
rozwiązywania konfliktów, amend ani commitów
w detached HEAD. Hooki pre-commit/prepare-commit-msg/commit-msg/post-commit
uruchamia natywny runner opisany w [git-hooks.md](git-hooks.md). X.509 i SSH defaultKeyCommand nie są obsługiwane;
SSH wymaga jawnego user.signingKey. Signing jest kwalifikowany na macOS.

## Testy i GUI

Testy obejmują stage→diff→commit, częściowy staging, unstage, rename, binary,
discard, literalne nazwy z metaznakami, pusty indeks, anulowanie, błędny signer,
zmianę indeksu podczas podpisywania i watcher pomijający ignored pliki.

SSH podpis jest weryfikowany przy jednorazowym kluczu; osobny test sprawdza
zaszyfrowany klucz przez SSH_ASKPASS i klucz publiczny przez izolowany ssh-agent.
Jawnie uruchamiane testy GPG tworzą jednorazowy GNUPGHOME, sprawdzają podpis oraz
zaszyfrowany klucz przez gpg-agent i testowe pinentry. Nie używają kluczy użytkownika.

GUI sprawdzano w osobnym testowym bundle i bazie: otwarcie diffu, Stage all,
wiadomość i commit oraz restore pane'a diffu po restarcie. Nie zmieniano indeksu
ani historii repozytorium użytkownika.
`CANOPY_DATA_DIR` pozwala jawnie wskazać katalog bazy dla izolowanych testów;
bez tej zmiennej pozostaje standardowa baza Canopy Rust. Nie zastępuj HOME.

Podczas jednego pełnego przebiegu wystąpiła niestabilność istniejącego testu
PTY `direct_executable_receives_literal_arguments_without_shell_interpolation`.
Ponowny test samodzielny, cały zestaw terminala i końcowy pełny przebieg przeszły.
Nie zmieniano tego testu ani logiki wyjścia PTY w ramach Git Changes.

## Historia bieżącej gałęzi

Changes → History odczytuje wyłącznie commity osiągalne z HEAD aktywnego worktree
(przy detached HEAD: z wybranego commita). Lista jest wirtualizowana, strony po
50 wpisów pobiera istniejący worker libgit2. Strony są zakotwiczone w tym samym
OID; Refresh/ponowne otwarcie historii pobiera aktualny HEAD. Zmiana worktree
czyści poprzednie dane; generacja odrzuca spóźnione odpowiedzi.

Kompaktowy wiersz zawiera graf, temat, hash i oznaczenie merge.
Po kliknięciu panel szczegółów wysuwa się na dole przez Presence/PANEL
(z Reduce Motion); zawiera autora, wiek, hash, wiadomość i liczbę rodziców.
Układ grafu powstaje przy odbiorze strony i zachowuje pasma między stronami.
Porządek topologiczny gwarantuje, że rodzic znajduje się za dzieckiem.
Graf pokazuje relacje rodziców, rozgałęzienia i merge w historii bieżącego HEAD.
Nie pokazuje nieosiągalnych branchy ani diffu wybranego commita.
Podgląd jest ograniczony do 10 000 wpisów, temat do 512 znaków, wiadomość do 8192.
Paginacja wykonuje ograniczony revwalk od zakotwiczonego HEAD; nie przechowuje
uchwytów repozytorium w encjach UI i nie odpytuje historii w pętli.

Pierwsze ładowanie historii pokazuje osiem statycznych skeletonów o wysokości
wiersza commita (32 px). Po odebraniu strony tylko nowe wpisy dostają fade
CONTENT_REVEAL z równomiernymi opóźnieniami w oknie maksymalnie pięciu tokenów STAGGER. Geometria
grafu nie jest przesuwana. Reduce Motion pomija fade i opóźnienia; po zakończeniu
nie ma żądań kolejnych klatek. Przy Load more dotychczasowa lista pozostaje widoczna.

Load more jest ostatnim wierszem wirtualizowanej listy, a nie stałą stopką
inspektora. Podczas doładowania zastępują go trzy skeletony na końcu listy;
po odpowiedzi nowe commity animuje BatchReveal, a kolejny przycisk pojawia się
z końcem tej animacji. Nie resetujemy pozycji scrolla przy dopisywaniu strony.

## Lokalne i zdalne referencje

Przy pierwszej stronie historii powstaje niezmienny snapshot HEAD, lokalnych
branchy i remote-tracking refs (do 4096 referencji), upstreamu oraz ahead/behind.
Kolejne strony współdzielą snapshot przez Arc, bez ponownego odczytu referencji
ani liczenia ahead/behind. Refresh tworzy nowy snapshot. Nie wykonujemy fetch.

Etykieta Local oznacza commit nieosiągalny z upstreamu, nie gwarancję, że commit
nie istnieje na żadnym innym remote. Szczegóły pokazują także commity osiągalne
z upstreamu. Przy braku upstreamu lub detached HEAD nie zgadujemy tego statusu.
Zestaw lokalnych commitów jest ograniczony do 10 000; po przekroczeniu limitu
nieoznaczone wpisy mają status nieznany, zamiast błędnie sugerować synchronizację.
Remote-only commity sygnalizuje licznik behind; nie rozszerzamy grafu poza historię
wybranego HEAD. Nazwy lokalnych branchy są niebieskie, remote-tracking — zielone.

### Tagi

Snapshot historii obejmuje również do 4096 lokalnych referencji refs/tags/*.
Tagi lightweight, annotated i zagnieżdżone annotated są przypisywane do końcowego
commita; tagi wskazujące blob/tree nie pojawiają się na timeline. Tagi mają żółte
etykiety i pierwszeństwo w kompaktowym wierszu; +N oznacza pozostałe referencje,
widoczne w szczegółach. Odczyt odbywa się raz na snapshot, poza renderem.
Nie zmienia to polityki fetchowania tagów podczas Pull.
