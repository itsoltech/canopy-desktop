# Typowane API v1 — wdrożony zakres

`proto/canopy/api/v1/api.proto` jest ręcznie utrzymywanym, samodzielnym kontraktem.
`build.rs` generuje klientów, serwisy i deskryptor `canopy-api-v1.bin`. Pakiet nie
importuje poprzedniego pakietu. Wszystkie usługi i klienci desktop/mobile używają
`canopy.api.v1`; usunięto poprzedni proto, codegen i adaptery zgodności.
Usługi delegują typowane intencje do wspólnej szyny i istniejących właścicieli.

## Usługi i adresowanie

| Usługa | RPC | Nazwa z rejestru |
| --- | --- | --- |
| Catalog | `ListThreads` | Odczyt bez inboxu |
| Catalog | `RenameThread` | `chat.rename` |
| Catalog | `ReadCatalog`, `ListResources`, `WatchSummaries`, `ListAgents`, `GetAgentCatalog`, `ListPullRequests`, `ReadThreadPage`, `PreflightClose` | Odczyty istniejących właścicieli |
| Catalog | `CreateThread`, `CloseThread`, `OpenProject` | `chat.create`, `chat.delete`, `projects.open` |
| Catalog | `PreflightCloseProject`, `PreflightCloseTerminal` | Odczyty istniejących właścicieli lifecycle |
| Catalog | `CloseProject`, `CloseTerminal` | `projects.close`, `terminal.close` |
| Thread | `SubscribeThread`, `ReadHistoryPage`, `SearchHistory`, `GetAgentOptions`, `PreviewEdit` | Odczyty istniejących właścicieli |
| Thread | `Start`, `Interrupt`, `Restart`, `Steer`, `RespondDecision` | `chat.start`, `chat.interrupt`, `chat.restart`, `chat.steer`, `chat.respond` |
| Thread | `SetAgentOption`, `SetThreadAgent`, `SetGoal`, `PauseGoal`, `ResumeGoal`, `ClearGoal` | `chat.set_option`, `chat.set_agent`, `chat.set_goal`, `chat.pause_goal`, `chat.resume_goal`, `chat.clear_goal` |
| Thread | `StopBackground`, `EditMessage`, `RunShell`, `StopShell`, `ForkThread` | `chat.stop_background`, `chat.edit`, `chat.shell`, `chat.stop_shell`, `chat.fork` |
| Thread | `SendMessage` | `chat.send` |
| Thread | `EditQueueMessage` | `chat.queue.edit` |
| Thread | `MoveQueueMessage` | `chat.queue.move` |
| Thread | `RemoveQueueMessage` | `chat.queue.remove` |
| Thread | `PauseQueue` | `chat.queue.pause` |
| Thread | `ResolveUnknownQueueMessage` | `chat.queue.resolve_unknown` |
| Settings | `UpdatePreferences` | `settings.preferences` |
| Settings | `GetSettings` | Odczyt istniejącego właściciela konfiguracji |
| Settings | `SaveAgentDefinition`, `RemoveAgentDefinition`, `SaveProfile`, `RemoveProfile`, `SaveIntegration`, `RemoveIntegration` | `settings.agent.save/remove`, `settings.profile.save/remove`, `settings.integration.save/remove` |
| Host | `GetInfo`, `WatchHost`, `ListDevices`, `ReadDiagnostics`, `GetCommand` | Odczyty z bieżącą autoryzacją |
| Host | `ResolveCommand`, `CloseCommandEpoch` | Wąski protokół utrzymania własnego inboxu |
| Host | `RevokeDevice` | `host.revoke` |
| Peer | `GetPeerRequest` | Istniejący dowód klucza wzajemnej tożsamości |
| Peer | `ConnectPeer` | `peer.connect` |
| Enrollment | `RequestPairing`, `GetPairingStatus` | Istniejący protokół jednorazowego zaproszenia, CSR i lokalnego zatwierdzenia |
| Files | `ListDirectory`, `SearchFiles`, `ReadFile` | Odczyty istniejącego właściciela plików |
| Files | `SaveFile`, `CreateFile`, `SetDirtyPresence` | `files.save`, `files.create`, `files.presence` |
| Git | `Preflight`, `GetStatus`, `ReadDiff`, `ReadHistory`, `ListRefs` | Odczyty istniejącego właściciela Git |
| Git | `Stage`, `Unstage`, `Discard`, `Commit`, `Pull`, `Push`, `ChangeWorktree` | `git.stage`, `git.unstage`, `git.discard`, `git.commit`, `git.pull`, `git.push`, `git.worktree` |
| Terminal | `ReadHistory`, `TerminalSession` | Istniejący viewport, historia i protokół wejścia PTY |
| Terminal | `OpenTerminal`, `AcquireInput`, `ReleaseInput`, `StopTerminal`, `RestartTerminal` | `terminal.open`, `terminal.acquire`, `terminal.release`, `terminal.stop`, `terminal.restart` |
| Artifacts | `GetUpload`, `PutChunk`, `Download` | Ograniczony transport bulk istniejącego magazynu |
| Artifacts | Osobne `Begin`, `Finish`, `Cancel` dla `ImageUpload`, `FileUpload`, `TaskUpload` | `artifacts.image.begin/finish/cancel`, `artifacts.file.begin/finish/cancel`, `artifacts.task.begin/finish/cancel` |
| Tasks | Odczyty kont, projektów, zadań, filtrów, formularzy, statusów, sprintów i załączników | Istniejący właściciel integracji i providerów |
| Tasks | `ApplyMutation`, `UpdateTask`, `SaveFilter`, `RemoveFilter`, `ConfigureTaskProject`, `TaskToWorktree` | `tasks.mutate`, `tasks.update`, `tasks.filter.save/remove`, `tasks.configure`, `tasks.worktree` |

Każda komenda zwraca wspólny `CommandReceipt` i opcję `command` z nazwą, ryzykiem
i listą nazwanych `Scope` zgodnymi z bitami rejestru Rust. Złożony scope shell
to CHAT i TERMINAL; odpowiedź na decyzję wymaga APPROVE. Nie ma `Execute`.
Receipt ma typowany wynik:
nazwę i rewizję, ID wiadomości, rewizję kolejki albo zatwierdzone preferencje
i rewizję wspólnego właściciela Settings.
Zapis pliku zwraca `FileResult` z hashem rzeczywiście zapisanych bajtów.
Git zwraca `GitResult` z wynikiem istniejącego workflow, w tym ostrzeżeniami hooków.

`ThreadRef` zawiera HostId i rzeczywisty UUIDv7 ThreadId. `ThreadTarget` dodaje
epokę executora. `ListThreads` zwraca zapisane wartości, rewizję i nazwę.
Opcjonalny `catalog_id` ogranicza odczyt do jednego nieprzezroczystego klucza
dotychczasowego katalogu. Łączy starą projekcję odczytową z nowym adresem;
nigdy nie jest parsowany jako ThreadId ani przyjmowany jako cel komendy.
Katalog filtruje wyniki według bieżących uprawnień i ogranicza odpowiedź limitem
control. `page_token` jest kursorem keyset po katalogowym kluczu: domyślnie
100, najwyżej 200 wyników, najwyżej 16 zapytań po 64 rekordy na stronę.
Kursor przesuwa się także po niedostępnych wierszach; strona może być pusta
i mieć kontynuację. Klient czyta do braku tokenu z budżetem 32 MiB.
Nie powstaje nieograniczony snapshot bazy ani replika.

Komenda wątku nie przyjmuje równolegle projektu lub worktree. Projekt, checkout
i korzeń załączników rozwiązuje host z metadanych wątku po sprawdzeniach busa.
Kontekst Send zachowuje konto, profil i epokę historii. Plik wskazuje ścieżkę
względną oraz wybór plików projektu albo prywatnego wątku. Pliki projektu nadal
wymagają READ oprócz CHAT. Konteksty i retencja obrazów mają istniejących właścicieli.

`EnvironmentRef` adresuje hosta ustawień, otwieranie projektu i tworzenie prywatnego
wątku. Tworzenie w projekcie i discovery przyjmują `ProjectRef`. Katalog przenosi
rzeczywisty zapisany ProjectId, osobno od ścieżki i przejściowego klucza widoku.
`ProjectRef` definiuje klucz zasobów poza wątkiem. `checked_project_ref` przyjmuje
autorytatywny rekord projektu: Git wymaga worktree, brak Git zabrania go,
a checkout musi należeć do projektu. Walidacja nie otwiera filesystemu. Obecny
identyfikator checkoutu to katalogowy workspace ID, nie ścieżka ani uprawnienie.

Host i Enrollment również wymagają `EnvironmentRef`. Pairing zachowuje istniejący
token, dowód CSR i lokalną decyzję; nie przyjmuje komendy inboxu przed utworzeniem
urządzenia. Zakresy urządzenia i stan pairingu mają nazwane enumy.

`CloseCommandEpoch(expected_epoch)` i `ResolveCommand` są jedynymi mutującymi
metodami utrzymania inboxu poza rejestrem komend. Mają dedykowane requesty/odpowiedzi,
bez `CommandMeta`; test kontraktowy sprawdza tę konkretną parę. Close nie zapisuje
`accepted` we własnej zamykanej epoce. Pozostaje dostępne przy limicie, atomowo
zwiększa epokę i zwalnia zakończone wyniki; retry właśnie zamkniętej epoki zwraca
ten sam wynik, a starsze żądanie nie zwiększa jej ponownie. Accepted/Attempting
blokują zamknięcie. Unknown pozostaje czytelne także po zamknięciu starej epoki.

GetCommand jest odczytem. Get/Resolve mogą wskazać pełny zasób i epokę executora;
sam ID jest dopuszczalny tylko dla jednoznacznego autoryzowanego wyniku. Writer
sprawdza wszystkie zapisane scope/rooty oryginalnej operacji przed ujawnieniem
wyniku, niejednoznaczności lub konfliktu epoki. Adres wyniku odtwarza z zachowanego
pełnego klucza, bez wymagania żywego projektu. Resolve potwierdza wyłącznie
archiwizację `delivery_unknown`, zachowuje ten stan i adnotację `resolved_by_user`,
nie wykonuje skutku ponownie. NotFound/Retired są stanami odpowiedzi odczytu,
nie udawanymi receipt wykonania.
Oryginalne RPC i GetCommand współdzielą projekcję wyników Create/Fork/OpenProject.
Wynik utworzenia zachowuje alias i adres utworzonego wątku również po jego
usunięciu. Asynchroniczne odczytanie aliasu kończy się ponownym sprawdzeniem
uprawnień do zachowanej oryginalnej operacji, przed ujawnieniem wyniku.

RevokeDevice jest normalną komendą ADMIN/Private. Efekt revoke, odrzucenie Accepted
urządzenia, cleanup obecności oraz Succeeded zapisują się w jednej transakcji.
Brak urządzenia daje trwałe Rejected. Lokalny revoke korzysta z tego samego efektu
bez admission; pozostaje dostępny także przy pełnym lokalnym inboxie.

Peer używa EnvironmentRef i komendy ADMIN/Private. Przed efektem zachowuje
dotychczasową weryfikację klucza, certyfikatu i tunelu. Journal utrwala dokładne
ConnectPeer przed wysłaniem, również podczas zestawiania kanału. Utrata potwierdzenia
nie pozwala zastąpić zapisanej intencji aktualnym portem powrotnego tunelu.
Inne nierozstrzygnięte operacje pozostają dostępne do przeglądu po połączeniu.
Niepewny zapis rejestru daje DeliveryUnknown, bez automatycznego powtórzenia.

CloseProject i CloseTerminal wymagają ProjectRef oraz planu istniejącego właściciela.
Plan nadal wiąże urządzenie, zasób i jedną komendę, z dotychczasowym TTL.
CloseProject ponownie sprawdza obecność brudnych buforów przed dispatch i zachowuje
cleanup oraz stan closing przy błędzie. Zmienione warunki lub nieaktualny plan dają
trwały Rejected. Retry najpierw autoryzuje zachowany root i sprawdza fingerprint,
bez wymagania ponownego istnienia projektu lub terminala. Desktop zachowuje
oryginalne typowane RPC zamknięcia projektu w dotychczasowym journalu.
Po restarcie host odrzuca niewykonane `Accepted` lifecycle katalogu, których
poprzedni właściciel i preflight już nie istnieją. Retry zachowuje tę odmowę;
operacje po `Attempting` nadal mają `DeliveryUnknown`, bez ponownego efektu.
Rename dotyczy wątku; dotychczasowy host nie udostępniał rename projektu lub terminala.

Files używa `ResourceTarget` z jednym celem: `ProjectRef` albo `ThreadTarget`.
Wątek rozwiązuje checkout z własnych metadanych. Katalog publikuje do istniejącego
rejestru korzeni również ProjectId i informację o Git; walidacja celu nie otwiera
filesystemu. Nieprawidłowy host, projekt lub checkout kończy się statusem transportu
przed odczytem pliku. Bus zapisuje znormalizowany `FileCommand`, tożsamość artefaktu
i warunki, bez kodowania Protobuf ani kopiowania zawartości pliku do inboxu.
Przed lookupiem aktualnego katalogu writer sprawdza zapisany root, uprawnienia
i dedupe. Dlatego retry zakończonego zapisu lub create działa po zamknięciu projektu,
a usunięcie uprawnienia zabrania odczytu zarówno wyniku, jak i konfliktu fingerprintu.
Warunki create i hash zapisu sprawdzane pod istniejącą blokadą przed `attempting`
dają trwały `Rejected/conditions_changed`. Błąd po rozpoczęciu zapisu daje `Failed`
albo `DeliveryUnknown`; odłączenie RPC nie oddaje własności przyjętej operacji.
Klient utrwala dokładne RPC Files w istniejącym journalu i ponawia oryginalny cel.
Retry pozostawionego `Accepted` ponawia walidację celu i warunków pod tą samą
blokadą. Usunięty lub zmieniony projekt daje trwałą odmowę, zamiast pozostawiać
epokę zablokowaną przez niewykonaną komendę. Po skutku właściciel zapisuje znany
wynik także po zmianie grantu; odpowiedź ujawnia go dopiero po ponownym sprawdzeniu
aktualnych uprawnień do zapisanego zakresu.

Git adresuje wybrany checkout przez `ProjectRef` z obowiązkowym worktree.
Bus utrwala domenowy `GitCommand`: zamiar oraz snapshot HEAD, indeksu, plików
i ID zatwierdzonego preflightu. Fingerprint nie używa bajtów Protobuf.
Istniejący właściciel zachowuje plany, libgit2, hooki, blokady docelowych worktree
i koordynację procesów/edytorów. Zmiana preflightu lub snapshotu przed próbą
skutku daje trwały `Rejected/conditions_changed`. Klient Changes utrwala dokładne
nowe RPC w istniejącym journalu.

Host dopisuje pełną listę autorytatywnych korzeni źródła i celów do payloadu
istniejącego inboxu. Lista pochodzi wyłącznie ze zgodnego planu hosta, nigdy z
deklaracji klienta. Otacza niezmienioną intencję klienta, która nadal wyznacza
fingerprint; zachowuje pełny klucz dedupe. Payload i `accepted` zapisują się w
jednej transakcji istniejącego workera, więc nie potrzeba nowej kolumny ani tabeli.
Odczyt wyniku lub konfliktu i przejście do `attempting` sprawdzają aktualny grant
do każdego zapisanego korzenia, także po usunięciu docelowego worktree i planu.
Brak lub niepoprawny dawny zakres kończy się bezpieczną odmową, bez odtworzenia
skutku. Lokalne Changes i lifecycle worktree przechodzą przez tę samą szynę;
snapshot lokalny zachowuje dotychczasowy brak limitów zdalnego destructive scan.
Odzyskanie `Accepted` zachowuje pierwotny payload z pełną listą korzeni. Nie
zastępuje go aktualnym, częściowym zakresem. Utracony lub zużyty plan kończy próbę
trwałym `Rejected/conditions_changed`, bez odtworzenia operacji.

Mutacje Terminal adresują `ProjectRef` i stabilny ID terminala, bez ścieżki
przekazanej przez klienta. Host sprawdza metadane projektu przed lookupiem
terminala, a następnie zgodność jego autorytatywnego rootu. Rejestr, fingerprint,
pełny klucz dedupe i aktualne granty obsługuje ta sama szyna. Zapisany root pozwala
odczytać receipt po zamknięciu projektu bez ponownego tworzenia procesu.
`TerminalResult` zawiera terminal, run i opcjonalny lease. Retry Acquire zwraca
pierwotny lease; nie odbiera wejścia późniejszemu właścicielowi ani nie odnawia
dzierżawy. Aktualny run i fencing token nadal sprawdza właściciel PTY.
Precondition lease/run daje trwałą odmowę przed `attempting`. Osobne streamy
zachowują porcjowanie ramek, budżety, odwołanie autoryzacji, anulowanie i cleanup
ostatniego attachmentu. Bajty PTY, resize i renew nie trafiają do inboxu.
Lokalne otwieranie, restart, przejęcie wejścia i jawne zamknięcie używają tej samej
szyny. Private/scratch bez projektu i wątku używa EnvironmentRef wyłącznie dla
lokalnego principala. Zdalny grant Private nie dopuszcza tego wariantu; publiczne
RPC nie przyjmują EnvironmentRef ani dowolnej ścieżki terminala.
Pozostawiona komenda `Accepted` terminala jest po ponowieniu odrzucana pod
rezerwacją zasobu. Zgodnie z CMD-07 dawny terminal ani Stop nie tworzy nowego
skutku; użytkownik sprawdza stan i nadaje nowy ID. Zapisany `Attempting` po
restarcie pozostaje `DeliveryUnknown`, bez replay. Zakończone komendy nadal
zwracają swój pierwotny wynik, także po usunięciu zasobu.

Artifacts rozdziela trzy cele, aby zachować obecne uprawnienia: obraz wymaga CHAT,
plik FILES_GIT, a załącznik zadania TASKS. Obraz adresuje wątek, plik projekt albo
wątek, a zadanie projekt albo środowisko. Każda granica transferu ma receipt;
porcje bajtów nadal obsługuje istniejący magazyn z walidacją offsetu, hasha i bindingu.
Pierwotny Begin zwraca stabilny ID wyprowadzony z pełnego klucza komendy.
Finish/Cancel wskazuje ID oraz epokę tego Begin. Host sprawdza jego udany wynik,
pełny klucz zasobu i aktualny grant; klient nie dostarcza korzenia. Dzięki temu
nowy Finish działa po zamknięciu projektu, korzystając z hostowego rootu Begin.
Własny retry Finish/Cancel najpierw sprawdza swój zachowany zakres i dedupe,
więc nie wymaga dalszego istnienia rekordu Begin. Brak dowodu dla nowej próby
kończy się odmową, bez odtwarzania celu ze ścieżki. Dotychczasowy payload inboxu
już zawiera root i typowany wynik Begin, dlatego nie potrzeba nowego schematu.
Niekompletny upload daje trwały `Rejected` przed rozpoczęciem Finish.
Retry `Accepted` Begin ponownie sprawdza żywy cel przed dostępem do magazynu.
Finish/Cancel ponownie dowodzi autorytatywnego Begin oraz bindingu uploadu pod
dotychczasową blokadą. Brakujące warunki kończą przyjętą próbę trwałą odmową,
a ukończony upload projektu nadal można opublikować po zamknięciu projektu.

Pozostałe ustawienia używają `EnvironmentRef`, zakresu ADMIN/Private i komend
domenowych opartych na dotychczasowych edycjach konfiguracji. Sekrety mają
niezmienne wersje w dotychczasowym vaulcie. Ich identyfikatory zależą od klucza
uwierzytelnionego żądania i keyed fingerprintu treści; inbox zawiera wyłącznie
te niejawne identyfikatory. Znormalizowany fingerprint busa obejmuje domenową
edycję i warunki, bez treści sekretów lub bajtów Protobuf. Dotychczasowy gate
obejmuje staging, przyjęcie, zapis właściciela i cleanup. Zapisane i niepewne
operacje zachowują swoje wersje; cleanup rozpoznaje stary oraz środowiskowy klucz
bez zmiany tabeli. Nieaktualna rewizja wykryta przed dispatch daje trwałą odmowę.
Właściciel nadal atomowo sprawdza rewizję przy właściwej edycji.

Desktop zapisuje dokładne RPC ustawień w istniejącym vaulcie klienta, porcjami
mieszczącymi się w dotychczasowym limicie credentialu. Plik journalu zawiera tylko
losowe odwołania. Zablokowany lub niekompletny vault zabrania wysyłki i replay;
odtworzenie zachowuje pierwotne RPC, ID, środowisko, epokę i oczekiwaną rewizję.
Zmiana zakończonego wpisu na następną komendę usuwa poprzednie odwołania dopiero
po zatwierdzeniu journalu. Awaria procesu między stagingiem a zapisem journalu
może pozostawić nieużywaną wersję w systemowym magazynie klienta; nie odtwarza
ona efektu.

Desktop utrwala receipt Begin/Finish/Cancel przed zwróceniem błędu. Odczyt recovery
sprawdza pierwotny pełny klucz bez replay. `Rejected`/`Failed` pozwala jawnie usunąć
lokalny załącznik i przygotować nowy Begin; nigdy nie zmienia ID starej próby.
Udany Begin bez Finish pozostaje chroniony do ukończenia, jawnego `cancel_upload`
albo potwierdzenia niedostępności bulk. Cancel ma własny zachowany zamiar i receipt,
nie usuwa źródła. `NotFound` bulk nie oznacza sukcesu Finish ani braku jego skutku;
nie rozwiązuje `Accepted`, `Attempting` ani `DeliveryUnknown`.
Jawny retry zachowanego `Accepted` Finish/Cancel wysyła ten sam zamiar przed
sprawdzeniem bulk, aby brak uploadu mógł zakończyć tę komendę trwałą odmową.
Retirement jest zapisany osobno od receipt. Pozwala porzucić sam dawny Begin;
nie nadpisuje niepewnego wyniku Finish. Zapisane stare epoki nie blokują zamknięcia
bieżącej epoki, lecz ich niepewne źródła nadal nie podlegają cleanup.
Starszy draft bez typowanego `api_begin` nie jest ponawiany. Użytkownik może go
jawnie usunąć; do tego czasu journal i bajty źródła pozostają zachowane.

`ForkThread` przyjmuje wyłącznie źródłowy `ThreadTarget`, punkt historii i opcjonalny
zamiennik wiadomości. Host wyprowadza ten sam projekt i ten sam worktree ze źródła;
nie ma celu projektu, wyjątku D16 ani zmiany klucza dedupe. Cross-project i Private
destination nie należą do kontraktu. Prywatne źródło nie ma dozwolonego celu tego
RPC. Dotychczasowy selektor destination pozostaje w desktopie; dla zdalnego Fork
wybór innego checkoutu lub Private daje jawną odmowę przed dispatch, bez cichego
przekierowania do źródła. Lokalny flow zachowuje dotychczasowe zachowanie.
Bezpośrednie żądanie Fork prywatnego źródła zwraca trwały receipt `Rejected`,
bez kopiowania historii lub tworzenia wątku.

## Wykonanie i odzyskiwanie

Usługi współdzielą uwierzytelniony listener i `CommandBus`. Nie kopiują
autoryzacji, fingerprintu, dedupe ani polityki ryzyka. Settings zachowuje live owner
i jego dotychczasową rewizję. Mutacja należy do istniejącego `ResourceLocks`
i zadania runtime do publikacji wyniku; cancel RPC nie anuluje przyjętego skutku.
Katalog ponownie sprawdza grant po asynchronicznej konwersji tożsamości.
Zmiana autoryzacji kończy typowany stream; reconnect pobiera nowy snapshot.

Błąd uwierzytelnienia, formatu, tożsamości lub infrastruktury jest statusem gRPC.
Zmiana warunków domenowych w transakcji daje trwały receipt `rejected` z
`conditions_changed`. Bus wycofuje częściowe zmiany kolejki przed zapisem odmowy.
Dotyczy to również niezgodnej epoki executora: odmowa zachowuje pierwotny pełny
klucz, nie rozwiązuje załączników i nie uruchamia efektu. Bieżąca autoryzacja
poprzedza retry i ujawnienie konfliktu.
Ponowne użycie ID z inną intencją daje `ABORTED`. Retry odmowy zwraca ten sam wynik.

Klient kieruje mutacje wyłącznie do nowych usług. Istniejący dziennik
urządzenia zapisuje dokładne typowane żądanie przed dispatch: ThreadId, epokę
executora, command ID i command epoch. Odczyt stanu nie ponawia skutku. Jawny retry
zachowuje oryginalne RPC, bez ponownego wyszukiwania celu ani fallbacku.
Nowe pola dziennika są opcjonalne przy odczycie. Stare wysłane komendy bez adresu
wątku pozostają do przeglądu; klient odmawia replay zamiast dopisywać bieżącą
tożsamość. Niewysłany szkic otrzymuje adres przy pierwszym wysłaniu.
Get/Resolve po samym ID odrzucają niejednoznaczność.

Tasks zachowuje istniejące konta, konfigurację, providerów i właściciela zapisu.
Bus utrwala niemodyfikowalne powiązanie konta osobno od intencji. Lokalne załączniki
są przygotowywane w tle w istniejącym bulk store, a inbox widzi tylko ich tożsamość.
Zwrócona niepewność dostarczenia pozostaje DeliveryUnknown; nie jest potwierdzana
ani automatycznie odtwarzana. Lokalne edycje konfiguracji zapisują typowane różnice
z referencjami sekretów, bez ich plaintext.

PreflightClose/CloseThread obejmują usunięcie wątku, a projekty i terminale mają
dedykowane pary preflight/close. Desktop i mobile używają tego samego wygenerowanego
kontraktu; generowanie nie dowodzi działania natywnych modułów mobile ani GUI.
Nie dodano replay outboxu, replikacji, handoffu ani serwera MCP. Parytet rzeczywistej
listy narzędzi MCP pozostaje jawnym TODO następnego zadania, nie dowodem AC-36.

## Kontrola kontraktu

Test `encoded_commands_match_the_registry_exactly` dekoduje rzeczywiste opcje
metod z deskryptora. Minimalne projekcje `prost::Message` zachowują rozszerzenie
51000, które zwykłe `prost_types::MethodOptions` pomija. Test sprawdza dokładnie
jedno RPC na komendę, name/risk/scope, obowiązkową opcję i brak `Execute`.
Jawny TODO wymaga podłączenia rzeczywistego przyszłego rejestru narzędzi;
pusta lista nie jest dowodem parytetu.

CI na Linux uruchamia `buf lint` **1.57.2**, weryfikując SHA-256 binarium.
CI sprawdza również aktualność wygenerowanych schematów mobile; job desktopu
uruchamia testy biblioteki i binarium.
`buf.yaml` wyłącza zewnętrzny Health; lint obejmuje cały
nowy pakiet. Wyjątki `RPC_RESPONSE_STANDARD_NAME` i `RPC_REQUEST_RESPONSE_UNIQUE`
wynikają ze wspólnego `CommandReceipt`. CI odrzuca referencje usuniętego kontraktu
i obecność adapterów zgodności. `buf breaking` używa reguł FILE i porównuje z
niezmiennym tagiem pierwszego wydania `canopy-api-v1`. Przy pierwszym wydaniu należy
utworzyć ten tag i ustawić `CANOPY_API_V1_RELEASED=true`; brak bazowego tagu odtąd
blokuje CI. Obecnie API jest niewydane i nie tworzymy sztucznej bazy porównania.
Zmiana łamiąca wydany kontrakt wymaga pakietu v2.

## SIMPLE-01..03

| Konstrukcja | Wymaganie i powód wobec najprostszego ponownego użycia |
| --- | --- |
| Kontrakt i usługi | RPC-08..11: stare DTO nie mają ThreadId i epoki; nowe usługi dzielą listener, bus, worker i lifecycle zamiast kopiować handlery. |
| `ThreadTarget`, requesty i wyniki | CMD-01/12, RPC-09/13: utrwalają pełny adres próby; sam legacy ID pozwalałby zmienić cel po utracie stanu. |
| `ThreadPublisher` | RPC-03: callback do istniejącej publikacji runtime umożliwia adapter biblioteczny i testy bez binarnego AppState. Nie ponawia efektu. |
| `Control`, `CatalogCommand`, rozszerzenie `ThreadPublisher`, `CatalogPublisher` | BUS/RPC-03: stabilny zamiar i callbacki do istniejących handlerów runtime/katalogu; biblioteka nie przejmuje AppState, nie tworzy nowego executora ani inboxu. |
| Bezpośrednie projekcje API v1 | RPC-08/13: istniejący właściciel tworzy docelowe typy katalogu i wątku. Usunięto przejściowe `Location`, konwersje między kontraktami i ich dodatkowy budżet; pozostają granice snapshotu, cursory, hash i dotychczasowe limity strumieni. Brak dodatkowej repliki. |
| Rozwiązanie katalogowe busa | CMD-03/12: sprawdza zapisany root i zamiar przed lookup/probe; świeże metadane właściciela są oddzielone od fingerprintu, bez nowej tabeli i dodatkowego workera. |
| `ThreadSend`, `ThreadPath` | RPC-13: host rozwiązuje projekt po autoryzacji i dedupe; stary Send wymagał workspace od klienta. Pipeline kolejki i załączników pozostaje wspólny. |
| `FileCommand`, `FileContent`, `ResourceTarget` | CMD-02/03 i RPC-09/13: tożsamość projektu/wątku i znaczenie polecenia zamiast fingerprintu bajtów requestu; ponowne użycie istniejących plików, artefaktów, blokad i workera. |
| `GitCommand`, `GitIntent`, `GitSnapshot`, `SavedAuthority` | CMD-02/03/05: stabilny zamiar, zatwierdzony snapshot i trwałe uprawnienia wszystkich celów w istniejącym payloadzie; ponowne użycie preflightu, workera Git, hooków i lifecycle. |
| `TerminalCommand`, `TerminalResult` | CMD-02/03/12: typowany zamiar i pierwotny wynik lease bez replay PTY; istniejący właściciel procesu, fencing, streamy, historia i budżety pozostają wspólne. |
| `ArtifactCommand`, `UploadOrigin`, `ArtifactResult` | CMD-02/03/12: osobne zakresy uprawnień i trwałe granice transferu; dowód pierwotnego Begin zachowuje uprawnienia projektu po jego zamknięciu, bez nowej tabeli i klientowego rootu. |
| `SettingsCommand`, prywatny zapis żądania w vaulcie klienta | CMD-02/03/12 i SEC-06/07: domenowe edycje z niezmiennymi referencjami sekretów oraz dokładny retry bez sekretów w zwykłym journalu; dotychczasowy właściciel konfiguracji i oba vaulty pozostają wspólne. |
| `PeerConnection`, komendy close i osobne wyniki preflightu | CMD-02/03/12 i RPC-13: dokładny zamiar wzajemnej tożsamości lub zamknięcia zasobu, z zachowanym rootem i właścicielem efektu; bez nowego workera i polityki uprawnień. |
| Prywatny `ApiIntent`, pola dziennika | CMD-12: dokładne RPC w istniejącym magazynie zamiast drugiego inboxu lub odtwarzania adresu przy retry. |
| Projekcje deskryptora | RPC-11: istniejące prost/prost-types wystarczają, bez nowej zależności ani własnego parsera wire. |

Nie dodano zależności Rust, zmian Cargo.lock, schematu SQLite, workera ani frameworka.
