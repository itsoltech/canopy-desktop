# Pull, Push i upstream

Przyciski są wspólne dla Files i History w prawym inspectorze. Operacje dotyczą
aktywnego worktree, a zadanie zachowuje jego ścieżkę nawet po zmianie widoku.
Podczas operacji mutacje Git są zablokowane. Sukces pojawia się jako toast, a błąd pozostaje inline w obu
widokach; zakończenie odświeża status i historię.

## Publikowanie gałęzi

Bez upstreamu otwiera się modal POPOVER z wyborem skonfigurowanego remote,
polem nazwy gałęzi wstępnie ustawionym na lokalną nazwę i akcją Use local branch
name. Publish branch wysyła wskazany OID do refs/heads/<branch>, bez force.
Nieistniejąca gałąź remote powstaje podczas pushu. Upstream zapisujemy dopiero
po sukcesie; odrzucony push nie zmienia konfiguracji. Przy klonie single-branch
po sukcesie dodajemy brakujący fetch refspec nowej gałęzi.

Sprawdzamy wynik [push_update_reference](https://docs.rs/git2/0.21.0/git2/struct.RemoteCallbacks.html#method.push_update_reference),
nie tylko wynik transportu. Zmiana lokalnego HEAD/brancha lub adresu remote
po przygotowaniu operacji powoduje błąd. Jeśli push już się udał, a później
nie da się zaktualizować tracking ref/upstreamu, komunikat jawnie to rozróżnia.

## Pull

Fetch pobiera tylko wybraną gałąź do remote-tracking ref, bez tagów. Następnie
wykonujemy fast-forward albo pozostawiamy lokalny branch, jeśli jest aktualny
lub wyprzedza upstream. Rozbieżność wymaga jawnego merge/rebase poza tym UI.
Brak upstreamu pozwala go wskazać; dla pulla remote branch musi istnieć.

Przed fast-forward sprawdzamy stan repo i pliki. Checkout safe nie nadpisuje
ignored; HEAD i branch są blokowane na czas publikacji. Nie robimy stash,
reset --hard, force checkout ani automatycznego commita. Błąd checkout pozostawia
branch bez przesunięcia. Awaria publikacji ref po checkout jest jawnie zgłaszana.

## Transport i granice

Git2 0.21.0 ma włączone ssh/https; libgit2 i OpenSSL są budowane z vendored źródeł.
Git CLI ani shell nie wykonują operacji Git lub uwierzytelniania.

- SSH: najpierw agent dostępny dla procesu, potem istniejące domyślne pliki
  ~/.ssh/id_rsa, id_ecdsa i id_ed25519, każdy tylko raz. Towarzyszący plik
  `.pub` jest przekazywany razem z kluczem prywatnym, jeśli istnieje. Bundled libssh2 na
  Windows próbuje Pageant, a następnie named pipe usługi Windows OpenSSH
  (`\\.\pipe\openssh-ssh-agent`, ewentualnie `SSH_AUTH_SOCK`). Socket agenta
  utworzony wewnątrz Git Bash nie jest tym samym transportem. Klucze
  szyfrowane/custom/hardware wymagają odblokowania w obsługiwanym agencie.
  Nie implementujemy pełnej konfiguracji OpenSSH/core.sshCommand ani własnego
  formularza passphrase. Jeśli istnieje `~/.ssh/config`, błąd przypomina, że
  aliasy Host, User i IdentityFile nie są stosowane przez Canopy/libgit2.
- Username SSH pochodzi wyłącznie z remote/callbacka, z fallbackiem `git`.
  Ogólne `credential.username`, używane przez HTTPS, nie może zmienić konta SSH.
- HTTPS macOS: hasło/token z internet-password w Keychain dla hosta i użytkownika.
  Username musi pochodzić z URL albo credential.username. Przy
  credential.useHttpPath=true lookup obejmuje również ścieżkę repozytorium.
- HTTPS Windows: Generic Credential `git:https://host[:port]`; przy
  credential.useHttpPath=true także `/path`. Akceptujemy blob UTF-8 albo format
  UTF-16 używany przez zgodne wpisy GCM. Username pochodzi z konfiguracji, URL
  lub pola wpisu. Jawny username musi dokładnie odpowiadać polu wybranego wpisu,
  aby sekret innego konta nie został wysłany pod zmienioną nazwą. Błąd podaje
  oczekiwaną nazwę wpisu.
- Nie wywołujemy credential helperów i nie używamy tokenów integracji do Git.
- Certyfikaty i host keys pozostają weryfikowane przez transport; nie ma bypassu.
- Connect timeout 15 s, socket timeout 30 s, limit 120 s sprawdzany w callbackach.
  Anulowanie podczas zamykania aplikacji jest kooperatywne. Push zaakceptowany przez serwer nie jest cofany, a
  zablokowany transport może czekać do timeoutu. Quit czeka na zakończenie zadania.
- Pre-push/post-merge hooki blokują właściwą operację zamiast być pomijane.
- Brak force-push, rebase/merge, tworzenia remote w UI i obsługi osobnego push URL.
- Obecnie odczytujemy ogólne `credential.useHttpPath` z konfiguracji repozytorium.
  Sekcje URL-scoped `credential "https://…"` nie są jeszcze dopasowywane, więc
  nie deklarujemy pełnej zgodności lookupu z całym mechanizmem konfiguracji GCM.

Błędy uwierzytelniania zachowują stan ograniczonej kolejki poświadczeń poza
callbackiem. Dzięki temu końcowy ogólny błąd libgit2 nie usuwa informacji o
braku agenta i plików, wykrytym szyfrowaniu domyślnego klucza albo odrzuceniu
dostępnego nieszyfrowanego klucza przez serwer. Komunikat zawiera username oraz
kod i klasę git2, nie zawiera zawartości klucza i nie może kończyć się pustym
dwukropkiem. Błąd fetch następuje przed checkoutem i aktualizacją ref, więc nie
zmienia working tree.

## Weryfikacja

`tests/git_network.rs` używa wyłącznie tymczasowych lokalnych bare repositories:
publish z inną nazwą, upstream, kolejny push, pull FF, no-op, odrzucenie pushu,
divergence, dirty/ignored files, anulowanie przed transferem, stale HEAD,
zmieniony URL, hook i rozszerzenie single-branch fetch mapping.
Nie kwalifikuje rzeczywistego SSH agenta, host keys, Keychain ani Credential
Managera użytkownika. Target lookup HTTPS ma osobne testy czyste; Windowsowy
round trip używa unikalnego wpisu testowego, lecz wymaga natywnego wykonania.

GUI release sprawdzone na osobnej bazie CANOPY_DATA_DIR i lokalnym bare remote:
edycja nazwy, Use local branch name, Publish branch, weryfikacja OID/upstreamu,
ponowny Pull bez modalu i historia ze stanem ahead/behind 0/0. Nie dotykano
produkcyjnego remote. Narzędzie przechwytywania GUI czasem pokazywało starszą
klatkę; końcowy stan potwierdzono po natywnym resize okna oraz odczytem referencji.

Pull/Push oraz błędy operacji są w stałej stopce na dole inspektora,
w obu widokach Files/History. Stopkę oddziela górna linia i padding 12 px;
nie jest częścią przewijanej listy ani sąsiadem przełącznika Files/History.

### Regresja: pusty agent przy działającym git pull w terminalu

Transport wcześniej wielokrotnie próbował wyłącznie ssh-agent, ignorując
standardowy klucz z pliku. `ssh_credentials` dostarcza teraz ograniczoną kolejkę
agent → istniejące domyślne klucze. Zawartość kluczy nie trafia do logów.
Testy potwierdzają kolejność, klasyfikację metadanych szyfrowania, rozłączne
komunikaty błędów, wybór username i brak ponawiania odrzuconego agenta w pętli.
Domyślna gałąź WinCNG w `libssh2-sys 0.3.1` kompiluje ładowanie prywatnego RSA
z pliku bez `HAVE_LIBCRYPT32`, przez co zwraca `Method unsupported in Windows
CNG backend`. Szczegół ten był później zastępowany ogólnym błędem auth. Canopy
włącza teraz dla Windows istniejące funkcje `openssl-on-win32` i
`vendored-openssl` tej samej przypiętej wersji. Ten build nadal zawiera backendy
Windows OpenSSH named pipe i Pageant, a dodatkowo obsługuje pliki RSA/OpenSSH.
Użytkownik potwierdził następnie działający Pull na Windows dla tego samego
remote i domyślnego klucza RSA, który działał również w Git for Windows. Pageant,
agent-only, klucze szyfrowane, custom IdentityFile i inne serwery pozostają
osobnymi przypadkami kwalifikacji.
Read-only `ssh_connection_probe` został jawnie uruchomiony na wskazanym repo
użytkownika i potwierdził uwierzytelnienie oraz odczyt reklamowanych referencji.
Nie wykonywał fetch/checkout/push. Zwykłe testy pomijają tę próbę sieciową.

W czasie transferu Pull i Push są wyłączone. Stopka nie pokazuje przycisku
Cancel transfer i zachowuje stały układ. Anulowanie przy quit pozostaje mechanizmem
wewnętrznym, odrębnym od przycisku Cancel commit.
