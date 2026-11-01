# Wspólna magistrala komend

`src/command_bus.rs` realizuje pierwszy zakres BUS-01..04 i CMD-01..12 na
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
i epokę executora. Preferencje używają `EnvironmentRef` hosta. Rejestr przewiduje
też `ProjectRef`; w tym zakresie nie ma komend projektu. Niewdrożone komendy
zachowują namespace `resource=''`, `executor_epoch=0`. Epoka komend urządzenia
dalej chroni zamkniętą retencję; nie zastępuje epoki executora.

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

Incoming draft zachowuje referencje załączników i podpisany kontekst. Rozwiązanie
workspace, kontekstu i retencji obrazów następuje po dedupe. Treść pliku nie jest
rozwijana do fingerprintu; zniknięcie zasobu po sukcesie nie zmienia receipt retry.

Legacy `GetCommand` i `ResolveCommand` nie zawierają resource ani epoki executora.
Po sprawdzeniu uprawnień odrzucają niejednoznaczny command ID, zamiast wybierać
dowolny wiersz. Odzyskanie wyniku: ponowić oryginalne typowane RPC z tym samym
command ID. Jawny retry Send zachowuje ID i zapisany request; nie stosuje hedgingu.
Typowane komendy mają kontrakt [canopy.api.v1](api-v1.md). Stare odczyty
i pozostałe operacje pozostają dostępne na czas niewydanej migracji.

## Zapis i niepewność dostarczenia

Wdrożone efekty są zmianami SQLite. Efekt domenowy i `succeeded` commitują się
w jednej transakcji (CMD-08). Nieudany zapis receipt wycofuje nazwę, preferencje,
kolejkę i retencję obrazów; nie wolno publikować sukcesu po samym enqueue zapisu.
Nie powstaje drugi inbox, worker ani warstwa repozytorium.

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

Poza magistralą pozostają create/delete/fork/recover, start/interrupt/restart,
steer/respond/options/agent/goal/background/edit/shell, otwieranie/zamykanie
projektów, terminale, Files/Git, Tasks/integracje, profile/sekrety, administracja
urządzeniami i artefaktami oraz inne mutacje. Ich dotychczasowe ścieżki pozostają
czynne. Sekrety nie trafiają do plaintext inboxu. Pierwszy etap
[canopy.api.v1](api-v1.md) udostępnia osiem komend rejestru, odczyt tożsamości
wątków i parytet deskryptora z rejestrem. Pełne BUS-05/RPC-11 czeka na pozostałe
komendy i rzeczywisty rejestr narzędzi. Nie dodano outbox replay, replikacji ani handoff.

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
`canopy.api.v1` dla komend busa oraz legacy odczyty, projekcję, stream/pump, dziennik żądań i odzyskiwanie szkiców
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
Nie używa hostowego `edit_target`, receipt ani dodatkowych warunków idle/kolejki/decyzji.
Nieedytowalny wybór pozostaje cichym no-op. Anulowanie podglądu nie pozostawia
waitera komendy; odpowiedź należy do sesji. Przeniesienie lokalnej edycji do trwałej
komendy hosta jest późniejszą pracą. Grpc zachowuje trwałe Edit i odzyskiwanie wyniku.
Wspólne mapowanie `edit_reason` nadal odpowiada za tekst hostowych odmów edycji
i lokalną odmowę starego runu; kody receipt nie są prezentowane przez lokalny flow.

Wynik przygotowania określa moment czyszczenia composera: zwykły Local Send bez
odtwarzania obrazów czyści synchronicznie przed wysłaniem; zapisane obrazy najpierw
otrzymują dane. Local Steer czyści przed tym odczytem, Goal czyści i opuszcza tryb
celu od razu, a udany synchroniczny Shell czyści i opuszcza `!`. Grpc nadal czeka na
potwierdzenie swojej operacji. Local Edit pozycji kolejki czyści przed oczekiwaniem
na jej zwolnienie; Grpc zachowuje pole do odpowiedzi i chroni nowszy szkic.
Czyszczenie po `DraftConsumed` usuwa także badge'y
czekające na wstawienie, aby nie pojawiły się ponownie w pustym polu.
Przygotowanie nadal zachowuje cel sprzed async odczytu. Odrzucony spóźniony Steer
dołącza swój szkic do później wpisanej treści, zamiast nadpisywać nowszy composer.

Istniejące ograniczenie: nieudane receipt edycji, które nie przechodzą przez
`acknowledge_runtime_command`, mogą pozostawać w `_canopy_command_inbox`.
Ten zakres nie zmienia cleanupu ani retencji inboxu.

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

Nie powstał worker, repozytorium, schemat ani zależność. Poza granicą pozostaje
administracja połączeń i projektowe Files/Git/terminale/tools. Wybór środowiska
w sidebarze i otwarcie lokalnej prezentacji zachowują metadane layoutu. Natywny
`ChatBridge` nadal obsługuje executorowe snapshoty/subskrypcje, katalog oraz
create/delete preflight hosta; nie przepuszcza lokalnego UI przez wire DTO.
Workspace nadal wybiera środowisko dla New Chat i New Tab z metadanych aktywnej
karty: na hoście tworzy czat przez wspólne `create_sibling`, a lokalny New Tab
zachowuje uruchomienie skonfigurowanego narzędzia. Pozostaje dotychczasowa kolejność
guardów modali i lokalnego workflow tworzenia; wybór ten nie wykonuje RPC.
Pozostała migracja kontraktu RPC i szersze API zasobów pozostają osobnym etapem.
