# Wspólna magistrala komend

`src/command_bus.rs` realizuje BUS-01..04 i CMD-01..12 na
istniejącym workerze SQLite. Desktop i gRPC przekazują typowany zamiar do tego
samego właściciela walidacji, autoryzacji, fingerprintu i deduplikacji. Odczyty
i operacje wyłącznie widoku nie korzystają z inboxu.

## Wdrożony zakres

| Komenda rejestru | Efekt i wynik receipt |
| --- | --- |
| `chat.rename` | Zatwierdzona nazwa katalogowa i rewizja wątku |
| `chat.send` | Przyjęcie wiadomości do trwałej kolejki; ID wiadomości |
| `chat.queue.edit`, `move`, `remove`, `pause`, `resolve_unknown` | Atomowa zmiana kolejki i jej nowa rewizja |
| `settings.preferences` | Patch preferencji, pełne zatwierdzone wartości i wynikowa rewizja ustawień |
| `chat.create`, `chat.delete`, `chat.fork` | Istniejący właściciel lifecycle; fork zachowuje źródłowy projekt/worktree i dedupe źródła |
| `chat.start`, `interrupt`, `restart`, `steer`, `respond`, `set_option`, `set_agent` | Przyjęcie przez bus, wykonanie przez istniejący runtime |
| `chat.set_goal`, `pause_goal`, `resume_goal`, `clear_goal`, `stop_background`, `edit`, `shell`, `stop_shell` | Istniejący handler runtime, trwałe stany i kontrola runu |
| `projects.open` | Istniejący właściciel projektów i jego persist; intencja adresowana środowiskiem |
| `projects.close`, `terminal.close` | ProjectRef, zachowany root i plan istniejącego właściciela; ponowna kontrola warunków przed skutkiem |
| `files.save`, `files.create`, `files.presence` | Adapter Files: kontrola hasha, create i atomowa obecność dirty; istniejący właściciel plików i blokad |
| `git.stage`, `git.unstage`, `git.discard`, `git.commit`, `git.pull`, `git.push`, `git.worktree` | Adapter Git: zatwierdzony preflight i snapshot, dotychczasowy libgit2, hooki i lifecycle; trwała odmowa zmienionych warunków przed próbą skutku |
| `terminal.open`, `terminal.acquire`, `terminal.release`, `terminal.stop`, `terminal.restart` | Adapter Terminal: projekt, run, token fencing i trwały wynik; procesy i wejście zachowują dotychczasowego właściciela |
| `artifacts.image.begin/finish/cancel`, `artifacts.file.begin/finish/cancel`, `artifacts.task.begin/finish/cancel` | Trwałe granice transferu, osobne uprawnienia CHAT/FILES_GIT/TASKS i hostowy dowód Begin; porcje bajtów pozostają u właściciela bulk |
| `settings.agent.save/remove`, `settings.profile.save/remove`, `settings.integration.save/remove` | Domenowe edycje istniejącego właściciela konfiguracji, ADMIN/Private; zamiar zawiera niejawne wersje sekretów, bez ich treści |
| `host.revoke` | ADMIN/Private; revoke i Succeeded w jednej transakcji istniejącego workera; lokalne odzyskanie administracyjne pozostaje dostępne bez admission |
| `peer.connect` | ADMIN/Private; dotychczasowy właściciel rejestru połączeń i credentiali; dokładny retry także przed publikacją kanału |

`Command`, `CommandEnvelope`, `Conditions`, `ResourceRef` i `Principal` nie są
typami Protobuf. `registry()` udostępnia nazwę, ryzyko i scope każdej komendy;
referencje plików workspace wymagają dodatkowo READ. Ryzyko jest punktem
rozszerzenia dla przyszłej polityki Ask. Obecny hook dopuszcza lokalnego właściciela
lub bieżące granty zdalnego urządzenia; nie dodaje dialogów potwierdzenia.
Lokalny właściciel jest kontekstem procesu, bez zdalnego credentialu. Principal
pochodzi z uwierzytelnienia adaptera, nigdy z pola żądania.

## Tożsamość, autoryzacja i retry

Pełny klucz inboxu to `(resource, executor_epoch, principal, command_id)`.
Dla wątku resource jest istniejącym `ThreadId`, a envelope zawiera `ThreadRef`
i epokę executora. Preferencje i open używają `EnvironmentRef` hosta. Create
używa `ProjectRef` albo środowiska dla Private. Epoka komend urządzenia
dalej chroni zamkniętą retencję; nie zastępuje epoki executora.

Projektowe komendy Files używają pełnego klucza `ProjectRef`. Pierwsze przygotowanie
sprawdza zapisany root przed lookupiem bieżącego katalogu, więc zamknięty checkout
nie blokuje retry zapisanego wyniku. `FileCommand` przechowuje względną ścieżkę,
hash bazowy lub tożsamość istniejącego artefaktu; zawartość pozostaje w bulk store.
Komenda Files wątku używa jego klucza i epoki. Atomowy zapis dirty presence sprawdza
ten sam znormalizowany zamiar oraz ponawia walidację epoki i autoryzacji w transakcji.
Lokalny edytor Save i Files Create przygotowują intencję w tej samej szynie;
zawartość pozostaje w istniejącym bulk store, a I/O u dotychczasowego właściciela.

Git zachowuje listę korzeni ustaloną przez zgodny preflight hosta w payloadzie
istniejącego inboxu, atomowo z `accepted`. Lista nie zmienia fingerprintu intencji
klienta ani klucza dedupe. Każdy zapisany root wymaga aktualnego grantu przed
wynikiem/conflict i przed dispatch, nawet po usunięciu worktree. Brak lub niepoprawny
zakres zapisanej próby zabrania replay. Nie ma nowej tabeli, kolumny ani workera.
Lokalne Changes oraz lifecycle worktree używają tej samej szyny i istniejącego
właściciela Git. Retry samego usunięcia gałęzi ma odrębną typowaną akcję;
cleanup brakującej rejestracji zachowuje jej nazwę i nie staje się usunięciem katalogu.

Terminal używa klucza projektu i zachowanego rootu przed lookupiem bieżącego
procesu. Acquire/Release mają typowany zamiar oraz receipt z pierwotnym lease.
Retry nie przejmuje wejścia ponownie i nie odnawia wygasłego lease. Streaming PTY
osobno sprawdza aktualny run, token i holder; jego bajty nie są kopiowane do inboxu.
Lokalne mutacje używają tej samej szyny. Private/scratch bez projektu i wątku ma
lokalny wariant EnvironmentRef; bus odrzuca go dla zdalnego principala niezależnie
od grantów. Publiczne RPC nie rozszerzają dostępu do Private. Katalogowe close projektu i terminala
korzystają z busa, zachowując obecne preflighty i cleanup. Rename projektu lub
terminala nie jest operacją dotychczasowego hosta; rename wątku ma własną komendę.
Lokalne `ForgetClosed` kończy lifecycle `chat.delete`: usuwa zachowaną historię
zamkniętego czatu i jego scratch. Wariant oraz ID czatu pozostają w typowanym
zamiarze i fingerprintcie. Nie jest to dodatkowa komenda publiczna; writer dopuszcza
tę akcję wyłącznie lokalnemu principalowi w środowisku.

Artifacts zachowuje root i wynik Begin w istniejącym payloadzie/receipt inboxu.
Nowy Finish/Cancel dowodzi pochodzenia uploadu pełnym kluczem udanego Begin;
po zamknięciu projektu host może użyć jego zapisanego rootu. Retry tej samej
komendy najpierw sprawdza własny zakres i dedupe. Nie ma nowego schematu ani
autoryzacji na podstawie klientowej ścieżki. Niekompletny Finish daje trwałą odmowę.

Settings zachowuje istniejący staging, gate i właściciela konfiguracji. Bus
normalizuje domenową edycję oraz warunki; wersje sekretów są keyed, a treść nie
wchodzi do inboxu. Dokładne żądanie klienta pozostaje w jego dotychczasowym
vaulcie, a zwykły journal zawiera tylko losowe odwołania. Zablokowany vault
zabrania wysyłki/retry. Nieaktualna rewizja wykryta przed dispatch daje trwały
`Rejected`; właściciel ponownie sprawdza rewizję atomowo przy edycji. Także jego
odmowa po fence `attempting`, gdy rewizja zmieniła się w międzyczasie, zapisuje
`Rejected/conditions_changed`: właściciel dowodzi, że edycja nie nastąpiła.

Ten sam UUID może niezależnie istnieć w dwóch wątkach, epokach executorów lub
u dwóch principals. Każdy odczyt, transition, ack i cleanup pojedynczego wpisu
używa pełnego klucza. Inbox nie ma cascade delete od katalogu wątków.
Trwały `thread_alias` pozwala odnaleźć receipt po usunięciu czatu.

Magistrala sprawdza bieżące uprawnienia przed ujawnieniem zapisanego wyniku
lub `idempotency_conflict` i ponownie przed dispatch. Sprawdza host, rodzaj celu,
epokę executora i warunki rewizji. Fingerprint SHA-256 obejmuje znormalizowaną
metodę, zasób, argumenty i warunki, bez bajtów Protobuf. Zapisany zamiar pozwala
odtworzyć te dane. Taki sam ID i zamiar zwracają zapisany stan; zmienione
argumenty lub warunki powodują konflikt.
Niezgodna epoka executora daje trwałe `Rejected/conditions_changed` pod pierwotnym
kluczem, tak jak przestarzała rewizja kolejki. Także zmiana rewizji między prepare
i przyjęciem przez runtime zapisuje odmowę przed odpowiedzią, bez efektu.

Incoming draft zachowuje referencje załączników i podpisany kontekst. Rozwiązanie
workspace, kontekstu i retencji obrazów następuje po dedupe. Treść pliku nie jest
rozwijana do fingerprintu; zniknięcie zasobu po sukcesie nie zmienia receipt retry.

GetCommand i ResolveCommand v1 opcjonalnie zawierają pełny zasób i epokę executora.
Po sprawdzeniu uprawnień odrzucają niejednoznaczny sam command ID, zamiast wybierać
dowolny wiersz. Odzyskanie wyniku: ponowić oryginalne typowane RPC z tym samym
command ID. Jawny retry Send zachowuje ID i zapisany request; nie stosuje hedgingu.
Typowane komendy i wszystkie odczyty mają kontrakt [canopy.api.v1](api-v1.md).
Poprzedni kontrakt, codegen i adaptery zgodności są usunięte z desktopu i mobile.

GetCommand nie przyjmuje komendy. ResolveCommand idempotentnie archiwizuje tylko
istniejący unknown, a CloseCommandEpoch zarządza własną retencją; obie metody
mają osobne odpowiedzi protokołu, bez nowego accepted i bez CommandMeta.
Ten jawnie testowany, wąski wyjątek wynika z §8.1 planu: przyjęcie Close do
zamykanej epoki samo blokowałoby jej zamknięcie. Nie obejmuje innych mutacji.
Read, lokalny revoke i cleanup zakończonych rekordów nie potrzebują wolnego miejsca
w inboxie. Revoke przez RPC podlega zwykłym limitom i atomowo wycofuje efekt,
jeśli zapis jego receipt jest niemożliwy.

## Zapis i niepewność dostarczenia

Efekty wyłącznie SQLite i `succeeded` commitują się
w jednej transakcji (CMD-08). Nieudany zapis receipt wycofuje nazwę, preferencje,
kolejkę i retencję obrazów; nie wolno publikować sukcesu po samym enqueue zapisu.
Nie powstaje drugi inbox, worker ani warstwa repozytorium.
Kontrole runtime i lifecycle zachowują swojego dotychczasowego właściciela:
bus przygotowuje oryginalny input, a istniejący handler ponownie sprawdza grant,
rewizję i epokę w transakcji acceptance. Efekty zewnętrzne mają `attempting`
przed skutkiem i `delivery_unknown` po niepewnej awarii; nie ma automatycznego replay.
Create/open sprawdzają zapisany zamiar i autoryzację przed ponowną rezolucją celu.
Zadanie aplikacji i `ResourceLocks` utrzymują operację po anulowaniu RPC.
Pozostawione `Accepted` nie jest końcowym wynikiem retry. Files i transfery
ponawiają walidację pod dotychczasową rezerwacją, Git zachowuje pełny zapisany
zakres i wymaga aktualnego planu. Brak celu lub planu daje trwałe odrzucenie.
Terminal odrzuca niewykonaną dawną komendę zgodnie z CMD-07; nowy skutek wymaga
sprawdzenia stanu i nowego ID. Restart nadal zmienia `Attempting` w
`DeliveryUnknown`, którego retry nie wykonuje skutku.
Po zakończeniu asynchronicznego skutku adapter ponownie autoryzuje zapisany
zakres przed odpowiedzią. Utrata grantu nie usuwa znanego wyniku z inboxu.

Sukces Send oznacza przyjęcie do kolejki, nie zakończenie wykonania. Istniejący
właściciel runtime osobno zapisuje `attempting` przed późniejszym dostarczeniem.
Restart/niepewna awaria pozostawia `delivery_unknown`, bez automatycznego replay.
Jawne resolve unknown zachowuje istniejące rozstrzygnięcia; Resend tworzy nową
pozycję kolejki atomowo w komendzie rozstrzygnięcia. Retry przyjętego Send nie
wysyła wiadomości ponownie ani nie wznawia kolejki wstrzymanej później.

Rename zatwierdza metadane przed publikacją w projektach i runtime. Opóźniony
zwykły save rekordu nie nadpisuje już jawnie zatwierdzonej nazwy.
Starszy lokalny pane bez katalogowej tożsamości przechodzi zwykły lazy save
katalogu przed rename; trwały alias jest odtąd używany ponownie. Zadanie lokalnej
komendy należy do ProjectsState i jest oczekiwane przy zamykaniu aplikacji.
Delete nieotwartego jeszcze lokalnego czatu również adoptuje istniejący pane
przed komendą. Odzyskanie kontekstu osieroconej historii jest lokalną typowaną
zmianą agenta: zachowuje ThreadId, historię i wybrany root w oryginalnym zamiarze.
Zmiana historii lub warunków odzyskania daje trwałą odmowę, a dokładny retry
zwraca pierwotny wynik także po zastąpieniu nierozstrzygniętego rootu.
Nazwy tabów ze splitami i etykiety lokalnych widoków zdalnych rozmów pozostają
stanem widoku; rename wątku hosta odbywa się przez jego komendę.

## Preferencje i wspólna rewizja

Lokalne `SettingsState::change`/`remember` oraz RPC UpdatePreferences używają
tego samego busa i właściciela rewizji w `SettingsState`. Rewizja nadal jest
losowa przy starcie i obejmuje zatwierdzone preferencje, narzędzia i integracje;
appearance, terminal, shortcuts i bookkeeping usuniętych credentiali są pomijane.
Rezerwacja właściciela obejmuje sprawdzenie CAS, commit i publikację. Konkurencyjny
zapis innego właściciela konfiguracji nie może zmienić tej granicy.

SQLite zapisuje patch i receipt z wartościami oraz wynikową rewizją w tej samej
transakcji. Retry zwraca dokładnie ten wynik, także po późniejszych zapisach
i restarcie; nie pobiera późniejszego snapshotu jako wyniku starej komendy.
Nieudany lub przestarzały CAS zachowuje wartości. Stan live jest publikowany
po commicie; zapamiętane lokalne zmiany są obsługiwane później.

## Schemat i pozostały zakres

Zmiany są częścią jedynej niewydanej `src/settings/database/runtime.sql` (D15).
Adopcja bazy sprzed runtime pozostaje transakcyjna. Prywatny prerelease o innym
fingerprincie nadal kończy się `UnsupportedSchema`; nie ma resetu ani automatycznej
aktualizacji niezgodnego schematu.

Wszystkie usługi używają [canopy.api.v1](api-v1.md). Tasks zachowuje niemodyfikowalne
powiązanie konta osobno od intencji i rozróżnia konkretny błąd od niepewnego
dostarczenia. Lokalne zapisy narzędzi, profili, integracji, filtrów i mapowań Tasks
przechodzą przez typowane różnice konfiguracji. Sekrety nie trafiają do plaintext
inboxu. Deskryptor jest porównywany z rejestrem komend; rzeczywisty rejestr MCP
pozostaje jawnym TODO kolejnego zadania. Nie dodano outbox replay, replikacji ani handoff.

Restart hosta kończy niewykonane `Accepted` komend katalogu trwałym
`Rejected/conditions_changed`: ich poprzedni właściciel i preflight już nie istnieją.
Operacje za granicą `Attempting` zachowują `DeliveryUnknown`. Retry odczytuje
te wyniki pod pierwotną tożsamością, bez nowego efektu.

## Dostęp do wątku: Local i Grpc

`app_state/chats/runtime/transport.rs` zawiera `ThreadTransport::{Local, Grpc}`,
poza modelem wątku. Wybór następuje z istniejących metadanych środowiska widoku;
trwałe aliasy katalogu i `PaneMetadata.remote` zachowują dotychczasowy format.
`ChatRuntime` pozostaje jedynym właścicielem wykonania. `Chats` ma wspólny rejestr
runtime i prezentacji, a transport rozstrzyga zwolnienie zasobu: lokalny właściciel
korzysta z dotychczasowego detach/delete i cleanupu, subskrypcja tylko się odłącza.
Kilka widoków tej samej referencji hosta współdzieli jedną subskrypcję.

Local bezpośrednio wywołuje istniejący bus, runtime, właściciela Settings i workery.
Nie koduje Protobuf i nie łączy się z własnym serwerem. Grpc skupia konwersje
`canopy.api.v1` dla komend busa, odczytów wątku/katalogu i discovery oraz
konwersję do istniejącego modelu domenowego, stream/pump, dziennik żądań i odzyskiwanie szkiców
w `transport/grpc.rs`. Krótkie odczyty mają właściciela anulowania zarówno po stronie
wywołującego, jak i subskrypcji. Mutacje nadal należą do istniejącego właściciela
operacji i trwałego dziennika; odłączenie widoku nie oznacza przerwania wykonania.

Wspólna granica obsługuje Send/Steer, kolejkę, lifecycle, decyzje, konto/opcje,
cele/background, shell, edit/fork, tworzenie obok czatu, transcript/snapshot/status/capabilities,
historię i wyszukiwanie, modele/konta i obserwację katalogu, sugestie, obrazy, strony
i pliki kart narzędzi. Wybór konta przy tworzeniu również otrzymuje domenowe `Account`.
Widok słucha tych samych zdarzeń runtime i korzysta z typów domenowych. Zapis
ostatnich wyborów Local nadal trafia przez `SettingsState::remember` do wspólnego
busa; CAS preferencji pozostaje u Settings/ControlBridge. Zmiana etykiety zdalnego
taba pozostaje operacją widoku; nie staje się hostowym Rename.
Lokalna edycja od razu otwiera wiadomość z pamięci; preview zmian plików dociera
osobno. Spóźnione odpowiedzi są sprawdzane względem runu i nadal otwartej edycji.
Local zachowuje dotychczasowe restore-then-fork: `fork_point` wybiera wiadomość,
sesja obsługuje dry run i właściwe przywrócenie, a `restart_edited` kontynuuje gałąź.
Nie używa hostowego `edit_target` ani dodatkowych warunków idle/kolejki/decyzji.
Nieedytowalny wybór pozostaje cichym no-op. Anulowanie podglądu nie pozostawia
waitera komendy; odpowiedź należy do sesji. Wysłanie Local Edit przyjmuje typowany
zamiar przed właściwym restore lub restartem. Istniejący właściciel sesji nadal
obsługuje restore-then-fork; jeden receipt obejmuje cały ten skutek. Zamknięcie
runtime zwalnia waitery i oczekuje jego zadań komend. Grpc zachowuje trwałe Edit
i odzyskiwanie wyniku.
Wspólne mapowanie `edit_reason` nadal odpowiada za tekst hostowych odmów edycji
i lokalną odmowę starego runu; kody receipt nie są prezentowane przez lokalny flow.

Wynik przygotowania określa moment czyszczenia composera: zwykły Local Send bez
odtwarzania obrazów czyści synchronicznie przed wysłaniem; zapisane obrazy najpierw
otrzymują dane. Local Steer czyści przed tym odczytem, Goal czyści i opuszcza tryb
celu od razu, a Shell czyści i opuszcza `!` po admission i uruchomieniu procesu.
Rewizja szkicu chroni treść wpisaną podczas oczekiwania. Grpc nadal czeka na
potwierdzenie swojej operacji. Local Edit pozycji kolejki czyści przed oczekiwaniem
na jej zwolnienie; Grpc zachowuje pole do odpowiedzi i chroni nowszy szkic.
Czyszczenie po `DraftConsumed` usuwa także badge'y
czekające na wstawienie, aby nie pojawiły się ponownie w pustym polu.
Przygotowanie nadal zachowuje cel sprzed async odczytu. Odrzucony spóźniony Steer
dołącza swój szkic do później wpisanej treści, zamiast nadpisywać nowszy composer.

Istniejące ograniczenie: nieudane receipt edycji, które nie przechodzą przez
`acknowledge_runtime_command`, mogą pozostawać w `_canopy_command_inbox`.
Lokalne znane wyniki są potwierdzane po trwałym zapisie; unknown pozostaje do oceny.

Wymagane nowe typy wynikają z D1 i SIMPLE-01..03:

| Typ | Konkretny powód zamiast najprostszego wcześniejszego rozwiązania |
| --- | --- |
| `ThreadTransport` | Dwie wymagane implementacje jednego dostępu; zastępuje `Option<RemoteSession>` oraz rozgałęzienia runtime/kolejki/widoku, bez traitów i frameworka |
| `Account` | Jeden katalog kont wykonującego środowiska; zastępuje składanie dwóch list w composerze |
| `HistoryPosition`, `HistorySearch` | Stronicowane trafienia i kotwice bez `ItemKey`/`HistoryMatches` w wyszukiwaniu widoku |
| `SavedMessageEdit` | Odtworzenie wcześniejszego composera i sprawdzenie bieżącego run bez wire preview w widoku |
| `MessageEdit`, prywatny `EditOrigin`, `MessageEditPreview`, `EditedBranch` | Wybór zachowuje punkt forka i kontekst Local lub adres hostowego Edit; zastępuje odczyt sesji i interpretację transportu w widoku |
| `EditProgress`, `EditSubmission` | Oddzielają przygotowanie obrazów, przywracanie, gotowość forka i oczekiwanie na receipt; widok zachowuje dotychczasowy moment czyszczenia i komunikaty bez wyboru transportu |
| `ComposerOperation`, `ComposerSubmission`, `ComposerClear`, `DraftPreparation` | Zamiast zawsze asynchronicznego submitu opisują faktyczną gotowość i moment czyszczenia; token zatrzymuje cel sprzed odczytu obrazów, UI nie zna jego tożsamości transportowej |
| `AttachmentSuggestion` | Wspólny wynik istniejącego wyszukiwania plików/PR; podpisany kontekst Grpc zostaje przy adapterze |
| `ThreadDestination`, prywatny `Destination` | Zachowanie adresu wybranego przed async fork; paleta nie interpretuje `WorkspaceRef` ani hostowego katalogu |
| `ContextReading`, `ContextSource` | Wspólny odczyt kontekstu z zachowaniem różnego opisu źródła; usuwa analizę wire transcriptu w UI |
| `PendingRequest`, `PendingOutcome` | UI otrzymuje treść i dozwolone akcje recovery, zamiast interpretować liczby receipt i prywatny dziennik hosta |
| `ThreadPlace`, `ThreadProject`, `ThreadFolder` | Fakty o miejscu wykonania bez projekcji wire; formatowanie ścieżek pozostaje prezentacją |
| `ThreadFile`, prywatny `FileOrigin` | Kliknięcie pigułki zachowuje autorytatywną lokalizację pliku; hostowa ścieżka nie staje się lokalnym uprawnieniem filesystemu |
| prywatny `ReadTask` | Zwykły Tokio JoinHandle odłącza zadanie przy drop; ta osłona anuluje odczyt także przed pierwszym poll i po zamknięciu subskrypcji |

Snapshot ponownie wykorzystuje istniejące `ChatSnapshot`, `ChatQueue` i `QueueEntry`.
Konstruktory `ChatQueue::observed` i `QueueEntry::observed` pozwalają adapterowi Grpc
odtworzyć tę domenową obserwację bez nowego DTO i bez udostępniania pól zapisu kolejki.
Nie nadają prawa do wykonania; mutacje nadal przechodzą przez bus. Nieznany stan
dostarczenia pozostaje `DeliveryUnknown`, więc nie staje się automatycznie ponawialny.

Nie powstał worker, repozytorium, schemat ani zależność. Administracja połączeń
i projektowe Files/Git/terminale/tools używają tej samej szyny. Wybór środowiska
w sidebarze i otwarcie lokalnej prezentacji zachowują metadane layoutu. Natywny
`ChatBridge` nadal obsługuje executorowe snapshoty/subskrypcje, katalog oraz
create/delete preflight hosta; nie przepuszcza lokalnego UI przez wire DTO.
Workspace nadal wybiera środowisko dla New Chat i New Tab z metadanych aktywnej
karty: na hoście tworzy czat przez wspólne `create_sibling`, a lokalny New Tab
zachowuje uruchomienie skonfigurowanego narzędzia. Pozostaje dotychczasowa kolejność
guardów modali i lokalnego workflow tworzenia; wybór ten nie wykonuje RPC.
Wszystkie adaptery korzystają z jednego natywnego kontraktu.
