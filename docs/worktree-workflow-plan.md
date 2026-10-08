# Ujednolicony workflow tworzenia i usuwania worktree

Status: plan zaakceptowanego kierunku produktu, do implementacji.
Data: 2026-09-13.

## Zadanie dla agenta

Zaimplementuj poniższy workflow w istniejącej aplikacji Rust + GPUI Kit.
Przeczytaj aktualne AGENTS.md i zachowaj niezwiązane zmiany w repozytorium.
Dokument opisuje docelowe zachowanie, nie deklaruje jego obecnego działania.
Wykonaj wszystkie trzy etapy wraz z dozwoloną weryfikacją; nie kończ na samym
formularzu lub backendzie. Nie commituj, nie pushuj i nie uruchamiaj GUI/E2E
bez odpowiedniego zlecenia lub zgody.

## Cel i decyzje produktu

- Użytkownik operuje nazwą brancha; Canopy zarządza nazwą katalogu worktree.
- Tworzenie ma jeden formularz, bez obowiązkowego kroku wyboru agenta.
- Usuwanie ma jeden formularz z wyborem: zachowaj branch, usuń lokalny branch,
  scal do innego brancha i usuń worktree.
- Domyślnym wyborem przy każdym usuwaniu jest zachowanie brancha.
- Merge musi się udać przed usunięciem worktree. Błąd lub konflikt nie uruchamia cleanupu.
- Operacje dotyczą lokalnego repozytorium. Nie dodajemy automatycznego fetch,
  push, usuwania zdalnych branchy, stasha, commita zmian użytkownika ani rebase.
- Zachowujemy wygląd Canopy, komponenty, tokeny i motion. Wytyczne skilla
  design stosujemy do hierarchii, formularzy i akcji; nie przenosimy webowego
  frameworka, fontów ani rozmiarów kontrolek do natywnej aplikacji.

## Punkty wejścia i aktualna baza

Zweryfikowane podczas przygotowania planu:

- `src/ui/worktree_dialog.rs`: wspólny dialog Create/Remove; obecnie edytowalny
  katalog `<repo>-<10 znaków UUID>`, nowy/istniejący branch, osobny krok agenta.
- `src/ui/sidebar/projects.rs`: tworzenie i usuwanie z sidebara.
- `src/ui/tasks_panel.rs`, `src/ui/task_detail/mod.rs`: tworzenie z Tasks.
- `src/app_state/git.rs`: orkiestracja Git, procesów i potwierdzeń.
- `src/git/mod.rs`: CreateWorktree, tworzenie, snapshot repozytorium.
- `src/git/removal.rs`: usuwanie katalogu i cleanup brakującego wpisu;
  obecna operacja zachowuje branch.
- `src/git/service.rs`: istniejący serializowany worker libgit2.
- `src/git/network.rs`: istniejące mechanizmy bezpiecznego fast-forward;
  Pull odrzuca divergence, więc nie jest gotową implementacją zwykłego merge.
- `src/app_state/editors.rs`, `src/app_state/terminals.rs`: ochrona buforów
  i cykl życia procesów. Korzystaj z nich zamiast tworzyć równoległe mechanizmy.
- `docs/git-worktrees.md`, `docs/task-worktree.md`, `docs/git-hooks.md`,
  `docs/git-changes.md`, `docs/persistence.md`: dokumentacja powiązana.

Ponownie sprawdź aktualny kod przed edycją; powyższa mapa nie zastępuje analizy
przepływu danych i może się zmienić między agentami.

## Etap 1: tworzenie worktree

### Formularz

Tytuł `Create worktree`. Kolejność:

1. `Branch name` — pole z początkowym fokusem.
2. `Start from` — wybór lokalnego brancha bazowego.
3. Dyskretna akcja `Use existing branch` przełączająca tryb.
4. Zwijane `Options`: agent/profil, domyślnie `No agent`.
5. Ścieżka katalogu jako tekst do skopiowania, bez edytowalnej nazwy.
6. `Cancel` i `Create worktree`.

Nie dodawaj konfiguracji katalogu nadrzędnego w pierwszej implementacji;
pozostaje obecna lokalizacja obok głównego repozytorium. Jest to opcjonalne
rozszerzenie, a nie warunek zakończenia zadania.

Domyślna baza: branch aktywnego worktree, jeśli należy do repozytorium formularza.
W przeciwnym razie branch głównego worktree. Jeśli żaden nie jest dostępny,
wymagaj jawnego wyboru; nie wybieraj przypadkowego pierwszego brancha.
Wybrana baza jest zawsze widoczna. Nie dodawaj obsługi unborn/bare repozytoriów,
jeżeli obecny backend jej nie zapewnia; pokaż konkretny powód niedostępności.

W trybie istniejącego brancha ukryj nazwę nowego brancha i bazę, pokaż
`Existing branch`. Przy branchu zajętym przez worktree pokaż informację oraz
`Open worktree`, które otwiera/aktywuje istniejący workspace zamiast tworzyć drugi.
Nie traktuj brakującego katalogu jako istniejącego worktree możliwego do otwarcia.

### Generowanie katalogu i zapis

- Wspólna logika domenowa generuje `<repo>-<10 znaków UUID>` poza istniejącymi
  working trees. UI jedynie prezentuje wynik, Tasks używa tej samej ścieżki.
- Propozycja pozostaje stabilna w obrębie formularza i podczas wpisywania nazwy.
- Ponownie sprawdzaj kolizję przy wykonaniu; wygeneruj nową nazwę, jeśli katalog
  został zajęty. Nie nadpisuj istniejących ścieżek ani symlinków.
- Operacja zwraca rzeczywistą ścieżkę. Nie używaj nazwy katalogu jako WorkspaceId
  ani nie zmieniaj katalogu przy późniejszej zmianie nazwy brancha.
- Zachowaj walidację Git branch name, zajętości brancha, dostępności lokalizacji
  i błędów częściowego tworzenia. Nie kasuj automatycznie plików po błędzie.
- Zapisz opcjonalną informację o bazie utworzenia przy trwałych metadanych
  worktree/workspace, zgodnie z istniejącymi właścicielami stanu. Dla nowego
  brancha zachowaj nazwę/ref i OID bazy; stare wpisy oraz istniejące branche
  mogą nie mieć tych danych. Migracja ma być addytywna, bez resetowania sesji.
- Baza jest podpowiedzią do przyszłego porównania, nie dowodem aktualnego merge.

### Agent i Tasks

Bez wyboru agenta powstaje pusty workspace. Jawnie wybrany agent uruchamia się
po utworzeniu zgodnie z obecną integracją. Tasks podpowiada branch, zachowuje
powiązanie i wstawia draft zadania do wybranego agenta bez Enter/wysłania.
Błąd startu agenta nie może udawać, że utworzenie worktree się nie udało,
ani przy ponowieniu tworzyć drugiego worktree.

## Etap 2: usuwanie i lokalny branch

### Formularz

X i menu kontekstowe otwierają ten sam `Remove worktree`.
Pokazuj branch jako główną tożsamość, poniżej ścieżkę oraz wyniki analizy.
Lista radio:

- `Keep branch` — zawsze domyślne.
- `Delete local branch`.
- `Merge into another branch, then remove` — realizowane w etapie 3.

Jeden główny przycisk odpowiada działaniu: `Remove worktree`,
`Remove worktree and branch` lub `Merge and remove`. Nie zapamiętuj wyboru
destrukcyjnego między operacjami.

### Analiza bezpieczeństwa brancha

Przy `Delete local branch` pokaż lokalny branch porównawczy, domyślnie zapisaną
bazę, jeśli nadal istnieje. Dla braku wiarygodnej podpowiedzi wymagaj wyboru.
Użytkownik może zmienić porównanie. Policz commity osiągalne ze źródła,
nieosiągalne z wybranego celu; komunikat musi wskazywać ten cel.

Zero takich commitów oznacza zawarcie historii w konkretnym celu, a nie
ogólne twierdzenie o zdalnym backupie. Squash/cherry-pick nie gwarantują relacji
przodków; nie rozpoznawaj ich jako bezpiecznego merge na podstawie podobnego diffu.
Przy niezerowej liczbie pokaż osobne potwierdzenie usunięcia lokalnego brancha
z nazwą oraz liczbą commitów niewłączonych do celu. Nie usuwaj commitów jako
obiektów Git i nie obiecuj ich bezterminowej odzyskiwalności.

Branch usuwaj dopiero po udanym usunięciu worktree, po ponownej kontroli ref/OID
i zajętości w innych worktree. Nie usuwaj brancha docelowego ani domyślnego
brancha repozytorium. Nie udawaj odczytu zdalnych reguł ochrony branchy.

### Potwierdzenia i kolejność

1. Odczyt stanu i wybór działania, bez mutacji.
2. Istniejąca ochrona niezapisanych buforów Save / Discard / Cancel.
3. Osobna zgoda na zatrzymanie procesów wskazanego workspace'u i oczekiwanie
   na PTY; dołącz do istniejącego cleanupu, nie uruchamiaj drugiego.
4. Ponowny status. Osobna zgoda na trwałe usunięcie local/untracked/ignored,
   z czytelnym podsumowaniem, także gdy część plików to artefakty builda.
5. Potwierdzenie utraty lokalnego brancha z niewłączonymi commitami, jeśli potrzebne.
6. Wykonanie wybranego działania; ponowna weryfikacja przed każdą mutacją.

Zgody wiąż z konkretnym zakresem, ścieżką, branchem i analizowanym stanem.
Nowe zmiany unieważniają nieaktualne zgody. Zachowaj ochronę przed double-clickiem
potwierdzającym dwa kolejne kroki. Anulowanie nie usuwa pozostałych danych,
nie odwraca zapisów buforów ani nie restartuje już zatrzymanych procesów.

Zachowaj blokady głównego worktree, locków, detached HEAD, submodułów oraz
niedokończonych operacji Git. Cleanup brakującego katalogu nadal jest osobną
operacją usuwającą wyłącznie rejestrację i zapisany workspace, z zachowaniem
brancha. Nie rozszerzaj jej na merge/usuwanie brancha w tym zadaniu.

## Etap 3: merge przed usunięciem

### UI i kontrakt

Po wyborze merge odsłoń `Merge into`, kierunek `source → target`, liczbę
commitów oraz wynik analizy: already integrated / fast-forward / merge commit /
conflicts / blocked. Cel: lokalny branch, różny od źródła; podpowiedz zapisaną
bazę, a w razie jej braku wymagaj wyboru.

Pokaż zaznaczony checkbox `Delete local branch after merge`; użytkownik może
go odznaczyć. Usuwanie brancha wciąż następuje dopiero po usunięciu worktree.
Already integrated pozwala przejść do cleanupu bez tworzenia pustego commita.

Merge obejmuje wyłącznie commity. Niezacommitowane zmiany źródła wymagają
uporządkowania przed rozpoczęciem tej ścieżki; pokaż `Open Changes`. Nie proponuj
ich automatycznego odrzucenia pod nazwą merge. Ignored/untracked pozostające
do usunięcia nadal wymagają właściwej zgody przed cleanupem.

### Wykonanie

- Zrób analizę konfliktów poza UI, bez modyfikacji indeksu, plików i refów.
  Przy konflikcie pokaż listę plików i możliwość przejścia do odpowiedniego
  workspace'u. Zachowaj oba branche i worktree; nie rozpoczynaj destrukcyjnej fazy.
- Obsłuż fast-forward oraz zwykły merge commit przy divergence. Nie zmieniaj
  kontraktu Pull, który nadal pozostaje fetch + fast-forward.
- Jeżeli cel jest checkoutowany, aktualizacja musi spójnie objąć jego ref,
  indeks i working tree. Sprawdź dirty buffers, staged/unstaged/untracked/ignored,
  stan Git i procesy. W pierwszej wersji blokuj merge do celu z działającymi
  procesami Canopy, kierując użytkownika do jego jawnego Stop; nie zatrzymuj
  innego workspace'u w tle. Brak checkoutu nie wymaga tworzenia widocznego workspace'u.
- Nie deklaruj wykrywania procesów spoza Canopy. Libgit2 safe checkout oraz
  ponowna kontrola refów i indeksów muszą chronić przed wyścigami.
- Dla merge commita zachowaj istniejący kontrakt podpisów, właściwe hooki merge
  i publikacji oraz ochronę przed zmianą HEAD/indeksu podczas hooka/pinentry.
  Sprawdź semantykę hooków merge; nie kopiuj bez analizy sekwencji zwykłego commita.
  Błąd podpisu nie ma unsigned fallbacku. Błąd hooka po publikacji jest ostrzeżeniem.
- Przed publikacją ponownie zweryfikuj źródłowy i docelowy OID oraz stan celu.
  Zmiana unieważnia analizę; nie nadpisuj cudzej aktualizacji.
- Kolejność: merge → potwierdzony cleanup worktree → opcjonalny lokalny branch.
  Wstrzymaj ponowne uruchamianie procesów źródła podczas tej operacji.

Nie dodawaj edytora konfliktów. Konflikty wykryte w analizie użytkownik rozwiązuje
poza tym formularzem, po czym wraca i ponawia analizę.

## Własność, błędy i cykl życia

- Git wykonuje istniejący ograniczony worker libgit2, nigdy render ani shell.
- Rozdziel formularze i analizę domenową zgodnie z istniejącymi modułami;
  nie rozbudowuj monolitycznie renderu WorktreeDialog.
- Wynik analizy, wymagane potwierdzenia i wynik wykonania są typowane.
  Nie parsuj tekstu błędu, aby ustalić następny krok.
- Wynik operacji rozróżnia co najmniej: merge niewykonany/wykonany,
  worktree zachowane/usunięte i branch zachowany/usunięty.
- Merge udany + cleanup nieudany: pokaż rzeczywisty sukces merge i błąd cleanupu;
  ponów tylko cleanup po ponownej kontroli. Nie wykonuj ponownie merge.
- Worktree usunięte + branch zachowany przez błąd: usuń nieaktualny workspace
  z katalogu aplikacji, pozostaw czytelny komunikat i możliwość ponowienia
  usunięcia dokładnie tego brancha po ponownej analizie.
- Nie obiecuj atomowości całej sekwencji ani rollbacku opublikowanego merge.
- Po restarcie odczytuj rzeczywisty stan Git; nie odtwarzaj ślepo zgód ani
  destrukcyjnych operacji. Spóźnione wyniki odrzucaj według tożsamości/generacji.
- Zachowaj blokadę operacji, kontrolowane anulowanie i oczekiwanie przy quit.
  Nie porzucaj trwającej mutacji przez zamknięcie modala.

## UI i dostępność

Wspólne modal helpers, form_field, dropdown, checkbox, przyciski z ButtonLoading.
Szerokość około 480 jednostek logicznych, ograniczona rozmiarem okna; przy małej
wysokości przewijana zawartość z dostępnymi akcjami. Ścieżki i długie nazwy nie
mogą wypychać przycisków. Jedna główna akcja; destrukcyjne potwierdzenie ma
odpowiedni wariant. Błędy inline, końcowe lekkie sukcesy przez istniejące toasty.

Zachowaj focus trap, powrót fokusu, obsługę klawiatury, blokadę backdropu,
POPOVER i Reduce Motion. Podczas analizy pokaż loader i zablokuj wykonanie
z nieaktualnym wynikiem. Enter nie może przypadkowo zatwierdzić nowo pokazanego
potwierdzenia utraty danych. Unikaj dodatkowych kart i globalnych animacji layoutu.

## Weryfikacja i kryteria odbioru

Testy jednostkowe/domenowe na tymczasowych repozytoriach, przez libgit2:

- Tworzenie: nowy/istniejący/zajęty branch, niepoprawna nazwa, kolizja katalogu,
  symlink, odmowa dostępu, wspólny generator dla Tasks i sidebara.
- Persist/restore: opcjonalna baza, starsze dane bez bazy, brak migracji nazw
  istniejących katalogów, zachowanie pustych i istniejących layoutów.
- Usuwanie: domyślne zachowanie brancha, usunięcie po potwierdzeniu, commity
  niewłączone do celu, zmiana refa po analizie, branch checkoutowany gdzie indziej.
- Regresje: brak mutacji przed zgodą, zmiany po Stop, ignored/untracked,
  dirty buffers, missing registration, lock, main worktree, detached HEAD.
- Merge: already integrated, fast-forward, divergence bez konfliktu, konflikt
  bez mutacji, dirty/zajęty cel, aktualizacja checkoutowanego celu, wyścig OID,
  błąd hooka/podpisu i ostrzeżenie po publikacji.
- Częściowe sukcesy: merge OK/cleanup błąd, cleanup OK/branch błąd, ponowienie
  właściwego etapu, anulowanie i ochrona przed podwójnym wykonaniem.
- Agent: brak procesu domyślnie, jawny start, zachowanie Task draft bez wysłania,
  błąd startu po poprawnym utworzeniu bez ponownego tworzenia worktree.

Uruchom proporcjonalne testy jednostkowe, `cargo fmt --all -- --check` oraz
`cargo clippy --locked --all-targets -- -D warnings` zgodnie z RTK.md.
Nie uruchamiaj osobnego build/release ani GUI/E2E wbrew obowiązującej granicy
zgody; uzyskaj zgodę na sprawdzenia poza dozwolonym zakresem, jeśli jest wymagana.

Po dozwolonej implementacji i weryfikacji poproś o GUI/E2E. Plan scenariuszy:
oba punkty tworzenia, klawiatura, walidacja, rozwinięcie Options, długie nazwy,
małe okno, loading, szybki double-click, wszystkie trzy warianty usuwania,
anulowanie na każdym potwierdzeniu, konflikt, częściowy sukces, restore.
Tylko izolowana baza CANOPY_DATA_DIR i kontrolowane repozytoria; bez usuwania
prawdziwych worktree lub branchy użytkownika. Nie nazywaj testów modelu dowodem GUI.

Zaktualizuj dokumentację worktree, Tasks i persistence oraz kontrakt AGENTS.md
o opcjonalnym usuwaniu brancha: dotychczasowe bezwarunkowe „branch pozostaje”
ma pozostać prawdziwe dla domyślnej ścieżki i cleanupu brakującego wpisu.
Raport końcowy: co działa, wykonane sprawdzenia i ich wynik, ograniczenia,
pozostałe sprawdzenia wymagające zgody. Nie przedstawiaj planowanej części jako gotowej.
