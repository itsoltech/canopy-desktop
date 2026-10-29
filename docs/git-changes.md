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

## Układ panelu

Nagłówek pokazuje gałąź worktree, przełącznik Changes/History i odświeżenie
bieżącego widoku. Pod nim jest podsumowanie „N files” z sumą dodanych
i usuniętych linii oraz paskiem ich proporcji, a niżej filtr ścieżki i chipy
A/M/D/R (ponowne kliknięcie wybranego chipu zdejmuje filtr). Grupy Staged
i Changes pojawiają się tylko wtedy, gdy mają pliki. W stopce jest pole commita
z informacją o podpisie i przyciskiem „Commit N files”, upstream z Pull/Push
oraz pasek pull requestu.

Snapshot zmian zawiera liczbę linii każdego pliku (`ChangesSnapshot::lines`,
w kolejności `files`) oraz upstream z ahead/behind (`ChangesSnapshot::upstream`).
Liczby linii liczy libgit2 z dwóch diffów (HEAD↔index, index↔working tree)
w workerze Git. Pliki binarne i większe niż 512 KiB nie mają liczby, a przy
ponad 400 zmienionych plikach liczby są pomijane w całości, żeby odświeżenie
pozostało tanie.

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

## Pull requesty

`app_state/pull_requests.rs` (stan), `ui/pull_request.rs` (widok) i
`integrations/github/pulls.rs` (GitHub). Odpowiedniki Electronowych
`PullRequestActions`, `CreateTaskPRModal` i `PRDetailsModal`, w mniejszym zakresie.

Pod Pull/Push w Changes jest pasek PR gałęzi wybranego worktree. Gałąź pochodzi
z worktree w snapshotcie Git; detached HEAD i folder spoza Gita nie mają paska.
Repozytorium i konto GitHub ustala się tak jak dla Tasks (`IntegrationsState::github`):
override projektu, inaczej origin, oraz konto przypisane do właściciela.
Zapytanie GraphQL (`pullRequests(headRefName:)`) wybiera otwarty PR lub szkic,
a bez nich najnowszy. Wynik jest pamiętany per worktree i gałąź. Ponowne
zapytanie następuje po 60 s przy ponownym pokazaniu gałęzi, po Pull/Push, po
zmianie kont GitHub oraz po Retry.

Stany paska: sprawdzanie; brak PR → „Create pull request”; otwarty lub szkic →
karta (numer, tytuł, stan, gałęzie), kliknięcie otwiera szczegóły; scalony lub
zamknięty → karta i „Create pull request”; brak konta → odnośnik do
Preferences → Integrations; origin spoza GitHuba → informacja; błąd → komunikat
i Retry.

Tworzenie: okno z tytułem (domyślnie z nazwy gałęzi), opisem w Markdown,
gałęzią docelową (lokalne gałęzie repozytorium, najpierw baza utworzenia
worktree, potem main/master/develop) i opcją szkicu. Najpierw sprawdzany jest
dostęp do GitHuba, potem gałąź jest wypychana przez transport Git: z istniejącym
upstreamem na niego (PR wskazuje gałąź upstreamu), bez upstreamu jest publikowana
na `origin` (albo pierwszy remote) pod tą samą nazwą. Tylko push blokuje inne
operacje Git. Następnie `POST /repos/{owner}/{repo}/pulls`. Sukces zamyka okno,
pokazuje toast i ustawia PR w pasku. Błąd zostaje w oknie razem z wpisaną treścią.
Odrzucenia nazywają przyczynę (uprawnienia tokena „Pull requests: read and
write”, istniejący PR lub brak commitów), a niepewny wynik mówi, że PR mógł
powstać. Rozpoczętego tworzenia nie da się anulować; okno nie zamyka się w trakcie.

Szczegóły: numer, tytuł, stan, gałęzie, autor i opis w Markdown (GitHub zwraca
do 1500 znaków; dłuższy opis ma dopisek) oraz „Open on GitHub”.

Poza zakresem tego etapu (są w Electronie): reviewerzy i assignee, review i checki,
liczba zmian, merge/close, szablon PR z taska, znacznik PR przy worktree w sidebarze.

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

## Native remote Git service

`remote::resources::GitRpc` implements typed status, diff, history, refs, preflight
and mutation RPCs with libgit2 and the existing Git domain operations. It receives
the host's shared GitClient and resolved shell environment; it never runs the Git
CLI. Authentication and root authorization precede metadata access. Git operations
require the selected workspace itself to be a worktree root, not an authorized
subdirectory that would silently widen access to its parent repository.

Each mutation carries a five-minute, single-use preflight ID bound to the verified
device, workspace, normalized typed intent (including all approvals), and exact
HEAD/index/worktree snapshot. A reused command ID retrieves its durable result
even after that plan expires. A different command needs a new valid plan. Snapshot
capture checks twice and includes symbolic HEAD, refs/config, index bytes, file
contents/modes and ignored paths. It is bounded to 200,000 entries, 64 directory
levels and 256 MiB of hashed input per capture. A changed or oversized repository
gets an explicit refusal. Status returns its captured precondition; this alone is
not a destructive approval. `changes::edit_checked` checks again immediately
before index writes or discard effects. Canopy gates do not lock external Git
processes; the final check/publication interval remains a real filesystem boundary.

Discard checks persisted dirty warnings for each selected path and reports device
names and last-seen times, including offline warnings. Git history uses a
HEAD-anchored continuation (100 entries per page, existing 10,000-commit cap); diffs
stream bounded chunks and expose truncation. Ref and status responses enforce the
control-frame limit. Mutations run outside the metadata worker's global queue so
independent remote repositories can progress.

Local mutations share the same host gate (`app_state::file_ops::gate`, given to
`GitClient::use_host_gate` in `GitState::bind`). The worker reserves the canonical
checkout for stage/unstage/discard, commit with hooks, Pull/Push, worktree create
(repository and destination), removal/merge (the worktree and the merge target)
and branch deletion. A nested path is covered by its reserved parent. The worker
never waits for the gate: a checkout held by an editor save, an agent write or a
remote operation fails immediately with "Canopy is already changing this checkout",
so one remote operation cannot stall the single metadata worker. Remote callbacks
that already hold the root (`run_resource_operation`, worktree lifecycle) do not
reserve again. Gates exclude Canopy operations only, not external Git processes.

Accepted operations outlive the RPC connection; Attempting is durable before
side effects. Known results, commit IDs, warnings and partial worktree outcomes
are retained for receipt replay. Network errors with uncertain delivery are
DeliveryUnknown, not an automatic retry. Hooks retain the existing execution and
post-commit warning behavior. Signing requests/configuration currently return
`needs_host_interaction` before launching pinentry; macOS HTTPS likewise refuses
before potentially interactive Keychain lookup. No unsigned fallback or hidden
OS prompt is introduced. Native signing/auth qualification remains separate.

Worktree stop/removal/merge requires the application's `WorktreeLifecycle` owner:
preflight supplies process/editor/draft impacts, apply performs approved process
cleanup, and finished resumes cleanup handles/closes a removed workspace after
the Git result. Without that owner the operation refuses explicitly. Creation uses
an exact authorized destination and cannot silently relocate to a proposed sibling.
The existing removal analysis, branch checks, explicit discard/unmerged approvals,
partial-success reporting and libgit2 publication safeguards remain authoritative.
