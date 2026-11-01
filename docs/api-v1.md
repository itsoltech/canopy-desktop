# Typowane API v1 — wdrożony zakres

`proto/canopy/api/v1/api.proto` jest ręcznie utrzymywanym, samodzielnym kontraktem.
`build.rs` generuje klientów, serwisy i deskryptor `canopy-api-v1.bin`. Pakiet nie
importuje `canopy.remote.v1`. To pierwszy z trzech niewydanych etapów D14/D15;
pełne RPC-08 i usunięcie starego pakietu nastąpi po przeniesieniu pozostałych usług.

## Usługi i adresowanie

| Usługa | RPC | Nazwa z rejestru |
| --- | --- | --- |
| Catalog | `ListThreads` | Odczyt bez inboxu |
| Catalog | `RenameThread` | `chat.rename` |
| Thread | `SendMessage` | `chat.send` |
| Thread | `EditQueueMessage` | `chat.queue.edit` |
| Thread | `MoveQueueMessage` | `chat.queue.move` |
| Thread | `RemoveQueueMessage` | `chat.queue.remove` |
| Thread | `PauseQueue` | `chat.queue.pause` |
| Thread | `ResolveUnknownQueueMessage` | `chat.queue.resolve_unknown` |
| Settings | `UpdatePreferences` | `settings.preferences` |

Każda komenda zwraca wspólny `CommandReceipt` i opcję `command` z nazwą, ryzykiem
i scope zgodnymi z rejestrem Rust. Nie ma `Execute`. Receipt ma typowany wynik:
nazwę i rewizję, ID wiadomości, rewizję kolejki albo zatwierdzone preferencje
i rewizję wspólnego właściciela Settings.

`ThreadRef` zawiera HostId i rzeczywisty UUIDv7 ThreadId. `ThreadTarget` dodaje
epokę executora. `ListThreads` zwraca zapisane wartości, rewizję i nazwę.
Opcjonalny `catalog_id` ogranicza odczyt do jednego nieprzezroczystego klucza
dotychczasowego katalogu. Łączy starą projekcję odczytową z nowym adresem;
nigdy nie jest parsowany jako ThreadId ani przyjmowany jako cel komendy.
Katalog filtruje wyniki według bieżących uprawnień i ogranicza odpowiedź limitem
control. Nie tworzy repliki ani nowej subskrypcji.

Komenda wątku nie przyjmuje równolegle projektu lub worktree. Projekt, checkout
i korzeń załączników rozwiązuje host z metadanych wątku po sprawdzeniach busa.
Kontekst Send zachowuje konto, profil i epokę historii. Plik wskazuje ścieżkę
względną oraz wybór plików projektu albo prywatnego wątku. Pliki projektu nadal
wymagają READ oprócz CHAT. Konteksty i retencja obrazów mają istniejących właścicieli.

`EnvironmentRef` adresuje hosta ustawień. `ProjectRef` definiuje klucz zasobów poza
wątkiem; ten etap nie dodaje ich operacji. `checked_project_ref` przyjmuje
autorytatywny rekord projektu: Git wymaga worktree, brak Git zabrania go,
a checkout musi należeć do projektu. Walidacja nie otwiera filesystemu. Obecny
identyfikator checkoutu to katalogowy workspace ID, nie ścieżka ani uprawnienie.

## Wykonanie i odzyskiwanie

Usługi współdzielą uwierzytelniony listener i `CommandBus`. Nie kopiują
autoryzacji, fingerprintu, dedupe ani polityki ryzyka. Settings zachowuje live owner
i jego dotychczasową rewizję. Mutacja należy do istniejącego `ResourceLocks`
i zadania runtime do publikacji wyniku; cancel RPC nie anuluje przyjętego skutku.

Błąd uwierzytelnienia, formatu, tożsamości lub infrastruktury jest statusem gRPC.
Zmiana warunków domenowych w transakcji daje trwały receipt `rejected` z
`conditions_changed`. Bus wycofuje częściowe zmiany kolejki przed zapisem odmowy.
Ponowne użycie ID z inną intencją daje `ABORTED`. Retry odmowy zwraca ten sam wynik.

Klient kieruje migrowane mutacje wyłącznie do nowych usług. Istniejący dziennik
urządzenia zapisuje dokładne typowane żądanie przed dispatch: ThreadId, epokę
executora, command ID i command epoch. Retry i odzyskiwanie wyniku ponawiają
oryginalne RPC, bez ponownego wyszukiwania celu i bez fallbacku do starej mutacji.
Nowe pola dziennika są opcjonalne przy odczycie. Stare wysłane Send bez adresu
wątku pozostają do przeglądu; klient odmawia replay zamiast dopisywać bieżącą
tożsamość. Niewysłany szkic otrzymuje adres przy pierwszym wysłaniu.
Legacy Get/Resolve po samym ID nadal odrzucają niejednoznaczność.

Pozostałe odczyty, strumienie i mutacje korzystają ze starego kontraktu na czas
migracji. Implementacja mobile i odbiór RPC-07 pozostają odroczone; generowane
typy nie dowodzą działania mobile. Nie dodano replay outboxu, replikacji ani handoffu.

## Kontrola kontraktu

Test `encoded_commands_match_the_registry_exactly` dekoduje rzeczywiste opcje
metod z deskryptora. Minimalne projekcje `prost::Message` zachowują rozszerzenie
51000, które zwykłe `prost_types::MethodOptions` pomija. Test sprawdza dokładnie
jedno RPC na komendę, name/risk/scope, obowiązkową opcję i brak `Execute`.
Jawny TODO wymaga podłączenia rzeczywistego przyszłego rejestru narzędzi;
pusta lista nie jest dowodem parytetu.

CI na Linux uruchamia `buf lint` **1.57.2**, weryfikując SHA-256 binarium.
`buf.yaml` tymczasowo wyłącza stary pakiet i zewnętrzny Health; lint obejmuje cały
nowy pakiet. Wyjątki `RPC_RESPONSE_STANDARD_NAME` i `RPC_REQUEST_RESPONSE_UNIQUE`
wynikają ze wspólnego `CommandReceipt`. `buf breaking` ma reguły FILE, lecz nie
jest obecnie bramką CI. Od pierwszego wydania v1 musi porównywać z wydanym
deskryptorem i blokować zmiany łamiące; taka zmiana wymaga pakietu v2.

## SIMPLE-01..03

| Konstrukcja | Wymaganie i powód wobec najprostszego ponownego użycia |
| --- | --- |
| Kontrakt i trzy usługi | RPC-08..11: stare DTO nie mają ThreadId i epoki; nowe usługi dzielą listener, bus, worker i lifecycle zamiast kopiować handlery. |
| `ThreadTarget`, requesty i wyniki | CMD-01/12, RPC-09/13: utrwalają pełny adres próby; sam legacy ID pozwalałby zmienić cel po utracie stanu. |
| `ThreadPublisher` | RPC-03: callback do istniejącej publikacji runtime umożliwia adapter biblioteczny i testy bez binarnego AppState. Nie ponawia efektu. |
| `ThreadSend`, `ThreadPath` | RPC-13: host rozwiązuje projekt po autoryzacji i dedupe; stary Send wymagał workspace od klienta. Pipeline kolejki i załączników pozostaje wspólny. |
| Prywatny `ApiIntent`, pola dziennika | CMD-12: dokładne RPC w istniejącym magazynie zamiast drugiego inboxu lub odtwarzania adresu przy retry. |
| Projekcje deskryptora | RPC-11: istniejące prost/prost-types wystarczają, bez nowej zależności ani własnego parsera wire. |

Nie dodano zależności Rust, zmian Cargo.lock, schematu SQLite, workera ani frameworka.
