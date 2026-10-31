# ADR 0001: Architektura wątków, szyny komend i replikacji

Status: Przyjęta, 2026-10-31.

Dokument zapisuje decyzje architektury dla [planu domenowego](../agent-runtime-remote-plan.md)
i [macierzy odbioru](../agent-runtime-remote-acceptance.md). Plan i macierz zostały
poprawione w tym samym kroku. W razie sprzeczności obowiązuje ten ADR, a dokument
wymagań wymaga poprawki.

`MUSI` oznacza warunek odbioru. `NIE MOŻE` oznacza zakaz.

## Kontekst

Plan z 2026-10-25 opisuje rozmowę jako zasób konkretnego hosta. Identyfikator `chat.id`
jest unikalny tylko na tym hoście. Kontrakt `canopy.remote.v1` adresuje zasoby przez
`ChatRef` i `WorkspaceRef`. Klient lokalny i zdalny mają osobne ścieżki w części kodu.

Plan wykluczał ogólną szynę komend. Jednocześnie wymagał wspólnych handlerów, dedupe,
autoryzacji i fingerprintu dla lokalnego desktopu i gRPC. Nowe wymaganie dodaje MCP
z pełnym zakresem operacji użytkownika. Trzy wejścia z osobnymi kontrolami dają trzy
miejsca rozbieżności.

Sparowane hosty mają widzieć wątki drugiego komputera także offline. Plan nie określał,
kto zapisuje metadane wątku, jak hosty wymieniają katalog i co dzieje się z intencjami
offline innymi niż tekst.

Kontrakt gRPC i jego proto nie są wydane. Komentarz w
`proto/canopy/remote/v1/canopy.proto` oraz `mobile/NATIVE-REMOTE.md` to potwierdzają.
Zmiana łamiąca kontrakt jest więc jeszcze możliwa bez migracji klientów.

## Decyzje

### D1 Wątek jako pełna jednostka pracy

Decyzja: Wątek (`Thread`) jest pełną jednostką pracy. Obejmuje środowisko, agenta,
gałąź lub worktree oraz metadane. Model nie ma podziału na wątki lokalne i zdalne.
Klient wybiera tylko transport: bezpośredni w procesie albo gRPC. Warstwy są
rozdzielone fizycznie i logicznie: wątek -> transport -> środowisko. Granicą
transportu jest `ThreadTransport` z implementacjami `Local` i `Grpc`.

Uzasadnienie: Ten sam wątek ma to samo zachowanie niezależnie od miejsca, z którego
go oglądamy. Podział w modelu podwaja kod UI, stanu i testów.

Odrzucone opcje: osobne typy `LocalChat` i `RemoteChat`; flaga `is_remote` w modelu.

Konsekwencje: UI i stan nie rozgałęziają logiki według transportu. Wątek nadal
ujawnia swoje środowisko w metadanych. `Local` nie koduje Protobuf (SIMPLE-05).

### D2 MCP na poziomie wątków z pełnym parytetem UI

Decyzja: MCP działa wyłącznie na poziomie wątków. Nie zależy od miejsca uruchomienia
wątku. Użytkownik i agent nadal widzą, gdzie wątek działa, oraz wszystkie jego
metadane. MCP udostępnia wszystko, co może zrobić użytkownik w UI, także konfigurację
aplikacji.

Uzasadnienie: Agent sterujący Canopy potrzebuje tych samych operacji co człowiek.
Częściowy zakres wymusza obejścia przez UI lub pliki.

Odrzucone opcje: osobny, wąski zestaw narzędzi MCP; MCP adresujący hosty zamiast wątków.

Konsekwencje: Lista narzędzi MCP pochodzi z rejestru komend (D3). Brak narzędzia
dla komendy UI jest błędem kontraktu, wykrywanym testem.

### D3 Jedna szyna komend

Decyzja: UI, gRPC i MCP używają jednej szyny komend. Szyna wykonuje autoryzację,
dedupe, fingerprint i klasyfikację ryzyka. Ustawienia są komendami. Komenda ustawień
zwraca wynik i nową rewizję.

Uzasadnienie: Jedno miejsce kontroli usuwa trzy kopie tych samych reguł. Wspólne
handlery bez wspólnej szyny nie gwarantują, że każde wejście sprawdza to samo.

Odrzucone opcje: wspólne handlery z kontrolami w każdym adapterze (dotychczasowy plan);
osobna ścieżka dla ustawień bez rewizji.

Konsekwencje: Adapter tylko dekoduje wejście i woła szynę. Rejestr komend w Rust
jest źródłem prawdy dla proto i MCP (D14). Wycofujemy zdanie planu, że ogólny
command bus nie jest wymagany.

### D4 Replikacja „A+”

Decyzja:

- Każde środowisko jest jedynym zapisującym dla swoich wątków i ich metadanych.
- Inne hosty wysyłają intencje przez istniejący outbox i inbox.
- Komendy mają dedupe i fencing przez `executor_epoch`. Rekord wątku ma `revision`.
- Katalog to strumień per środowisko z kursorem i snapshotem. Usunięcie wynika
  z braku rekordu w snapshocie (niejawny tombstone).
- Topologia to bezpośrednia siatka połączeń między sparowanymi hostami, bez relay.
- Transkrypt synchronizuje się od hosta wykonawczego.
- HLC z LWW obowiązuje tylko dla preferencji użytkownika bez właściciela.
- Klasy replay: metadane odtwarzane automatycznie; Send tylko jawnie; Git, Delete
  i Stop nigdy.

Uzasadnienie: Jeden zapisujący daje prosty porządek i brak konfliktów zapisu.
Istniejący inbox zapewnia już dedupe i stany wykonania.

Odrzucone opcje:

- CRDT stanu. Jest sprzeczne z SIMPLE-04.
- Multi-writer LWW metadanych wątku. Powoduje cichą utratę danych.
- Relay przez host pośredni. Wymaga dodatkowego zaufania i nie jest potrzebny w LAN/VPN.

Konsekwencje: Zmiana metadanych z obcego hosta jest intencją z oczekiwaną rewizją.
Konflikt rewizji jest jawny. Offline host nie może zmienić cudzego wątku lokalnie.

### D5 Stałe środowisko wykonawcze

Decyzja: Agent wątku działa w środowisku ustalonym przy utworzeniu wątku. To środowisko
się nie zmienia. Jawny handoff z fencingiem `executor_epoch` to późniejszy etap.

Uzasadnienie: Przeniesienie procesu, sesji dostawcy i worktree wymaga osobnego
kontraktu. Pierwszy etap tego nie potrzebuje.

Odrzucone opcje: automatyczny failover (SIMPLE-04); handoff w pierwszym etapie.

Konsekwencje: `executor_epoch` istnieje od początku, żeby handoff nie zmienił kluczy
dedupe. W pierwszym etapie epoka zmienia się tylko przy jawnych operacjach hosta.

### D6 Pełny zakres replikacji

Decyzja: Replikujemy wszystko, także pełny transkrypt i artefakty. Każdy sparowany host
ma replikę. Replika poza właścicielem jest tylko do odczytu.

Uzasadnienie: Offline odczyt i wyszukiwanie wymagają pełnych danych.

Odrzucone opcje: replikacja tylko katalogu i podsumowań; transkrypt na żądanie.

Konsekwencje: Rośnie zużycie dysku i sieci. Replika korzysta z tych samych tabel
historii co dane własne (DATA-05). Limity repliki są otwartą kwestią.

### D7 Tożsamość środowiska

Decyzja: Tożsamością środowiska jest tylko `HostId`. Projekt i worktree są metadanymi
wątku.

Uzasadnienie: Ten sam projekt na dwóch hostach to dwa środowiska wykonania.
Złożony klucz środowiska komplikuje adresowanie i migrację.

Odrzucone opcje: klucz `(HostId, ProjectId)`; klucz `(HostId, WorkspaceId)`.

Konsekwencje: API nie zawiera `WorkspaceRef` jako klucza wątku.

### D8 Globalny identyfikator wątku

Decyzja: Wątek ma nowy globalny identyfikator UUIDv7 (`ThreadId`). Stare ID mapuje
trwała tabela `thread_alias(legacy_host_id, legacy_chat_id) -> thread_id`.
Wymaga to feature `v7` crate'a `uuid`.

Uzasadnienie: `chat.id` jest unikalny tylko na jednym hoście. Replika na wielu hostach
potrzebuje klucza globalnego. UUIDv7 jest sortowalny po czasie utworzenia.

Odrzucone opcje: zachowanie `chat.id` jako klucza; para `(HostId, chat.id)` jako
jedyny klucz w całym systemie.

Konsekwencje: Migracja tworzy `ThreadId` dla każdej rozmowy i wpis aliasu.
Historia nie jest kopiowana. `Cargo.toml` dostaje feature `v7` przy wersji
przypiętej w Cargo.lock.

### D9 Praca offline

Decyzja: Offline klient czyta z repliki. Intencje (Send, metadane) trafiają do outboxu.
Replay działa według klas z D4.

Uzasadnienie: Użytkownik czyta i przygotowuje pracę bez sieci. Klasy replay chronią
przed niebezpiecznym powtórzeniem skutku.

Odrzucone opcje: blokada UI offline; automatyczny replay wszystkich intencji.

Konsekwencje: OFF-05 obejmuje Git, Delete i Stop jako klasę nieodtwarzaną.

### D10 Tożsamość i uprawnienia MCP

Decyzja: Każdy principal MCP ma osobną tożsamość urządzenia na każdym hoście już
w pierwszym etapie. Uprawnienia pochodzą z grantu OAuth połączenia. Grant ustala
Allow, Ask albo Deny dla każdego zakresu. Pełny dostęp admin działa bez potwierdzeń.
Tryb Ask wymaga potwierdzenia. Dotyczy to także konfiguracji wykonującej kod.

Uzasadnienie: Osobna tożsamość pozwala odwołać jeden principal bez skutku dla innych.
Dedupe i limity inboxu są per tożsamość.

Odrzucone opcje: wspólna tożsamość MCP dla całego hosta; użycie tożsamości urządzenia
użytkownika.

Konsekwencje: Inbox i limity CMD-11 dotyczą każdego principala osobno. Klasa ryzyka
komendy (D3) wskazuje, kiedy Ask wymaga potwierdzenia.

### D11 Widoczność wątków dla MCP

Decyzja: MCP widzi wszystkie wątki ze wszystkich sparowanych hostów w zakresie grantu.

Uzasadnienie: Wynika z D1, D2 i D6.

Odrzucone opcje: widoczność ograniczona do hosta, z którym połączony jest MCP.

Konsekwencje: Odczyt czyta replikę. Mutacja trafia do właściciela wątku przez szynę.

### D12 Klasy ustawień

Decyzja: Ustawienia hosta zostają na hoście. Ustawienia urządzenia są lokalne.
Preferencje użytkownika synchronizują się między hostami przez HLC z LWW już
w pierwszym etapie.

Uzasadnienie: Preferencje użytkownika nie mają właściciela. LWW jest tu akceptowalne,
bo zmiana jest jawna i nie usuwa danych pracy.

Odrzucone opcje: synchronizacja wszystkich ustawień; brak synchronizacji preferencji.

Konsekwencje: Każde ustawienie ma przypisaną klasę. Przypisanie wymaga listy
(otwarta kwestia).

### D13 Stan globalny i jedna instancja

Decyzja: Canopy ma jeden stan globalny aplikacji: projekty, wątki i układ. Okna tylko
go renderują. Jeden użytkownik ma jedną instancję aplikacji. Druga instancja nie może
pokazywać innego stanu. MCP nie adresuje okien. Akcje widoku zmieniają stan globalny.

Uzasadnienie: Dwa stany w dwóch instancjach dają dwóch zapisujących dla tych samych danych.

Odrzucone opcje: niezależne instancje na tym samym katalogu danych; adresowanie okien w MCP.

Konsekwencje: Stan okna jest częścią stanu globalnego. Zasięg blokady jednej instancji
przy `CANOPY_DATA_DIR` jest otwartą kwestią.

### D14 Kontrakt gRPC `canopy.api.v1`

Decyzja:

- Nowy typowany pakiet `canopy.api.v1` jest zbudowany wokół wątku i komend.
- Obecny `canopy.remote.v1` usuwamy w tym samym refaktorze. Kontrakt jest niewydany.
- Klucze: `EnvironmentRef{host_id}` i `ThreadRef{host_id, thread_id}`. `ChatRef`
  i `WorkspaceRef` znikają z API.
- Jedna komenda to jedno RPC zwracające `CommandReceipt`. Adapter tylko dekoduje
  i woła szynę. Nie ma generycznego `Execute`.
- Rejestr komend w Rust jest źródłem prawdy. Proto jest pisane ręcznie.
- Każde RPC komendy ma opcję `(canopy.api.v1.command) = {name, risk, scope}`.
- Test kontraktowy porównuje deskryptor proto, rejestr komend i listę narzędzi MCP.
- `buf lint` działa w CI. `buf breaking` blokuje od wydania v1. Zmiany łamiące
  wymagają pakietu v2.

Uzasadnienie: Typowane RPC dają walidację i generowanych klientów. Opcja na RPC wiąże
proto z rejestrem bez generowania proto z Rust.

Odrzucone opcje:

- Zachowanie obecnych usług. Daje to trwałą podwójną identyfikację (`ChatRef` i `ThreadRef`).
- Dwa kontrakty równolegle. Podwaja adaptery i testy.
- Generowanie proto z rejestru. Utrudnia kontrolę numerów pól.

Konsekwencje: Klient mobile i desktop przechodzą na nowy pakiet w tym samym refaktorze.
Nazwa pakietu nie zawiera słowa „remote”, bo transport nie należy do modelu.

### D15 Kolejność prac

Decyzja: Refaktor wątku i komend wykonujemy przed pierwszym wydaniem Remote.
Całość ma jedną migrację danych.

Uzasadnienie: Po wydaniu zmiana kluczy i kontraktu wymagałaby migracji klientów i v2.

Odrzucone opcje: wydanie Remote na obecnym kontrakcie i późniejsza migracja.

Konsekwencje: DATA-01 obejmuje także `ThreadId`, `thread_alias`, rewizje wątku
i stan replikacji.

## Wpływ na istniejące wymagania

| ID | Zmiana |
| --- | --- |
| §2 (granice) | Usunięte „Nie jest wymagany ogólny command bus”. Wymagana jedna szyna komend (BUS-01..05). |
| SIMPLE-05 | Lokalny desktop woła tę samą szynę przez transport `Local`. |
| §4 (pojęcia) | `Chat` zastąpiony przez wątek; dodane środowisko, `ThreadTransport` i `executor_epoch`. |
| §7 (dane) | Klucz historii `ThreadId`; dodane `thread_alias` i stan replikacji; inbox z `ThreadId`, `executor_epoch` i principalem. |
| §9.1 (usługi) | Pakiet `canopy.api.v1`; usługi Catalog i Thread zamiast Catalog i Workspace oraz Chat. |
| §15 (etapy) | Rejestr komend w E0; model wątku i szyna w E1 przed wydaniem Remote; replikacja i MCP w E3. |
| OWN-04 | Cel operacji to `(ThreadId, executor_epoch)` z chwili utworzenia. |
| OWN-09..11 | Nowe: stałe środowisko, stan globalny, jedna instancja. |
| DATA-04 | Nowy `ThreadId` UUIDv7 i trwały `thread_alias` zamiast zachowania `chat.id` jako klucza. |
| DATA-05 | Replika obcych wątków korzysta z tych samych tabel historii. |
| CMD-01 | Dedupe po `(ThreadId, executor_epoch, DeviceId/principal, command_id)` u wykonawcy. |
| OFF-01, OFF-03, OFF-05 | ThreadRef w outboxie; klasy replay z D4. |
| OFF-07 | Nowe: automatyczny replay metadanych z kontrolą rewizji. |
| RPC-01 | Adaptery wołają wspólną szynę. |
| RPC-08..12 | Nowe: `canopy.api.v1`, klucze, RPC na komendę, rejestr, `buf`. |
| SYNC-03 | Delta adresuje `ThreadId` zamiast `ChatId`. |
| REPL-01..07 | Nowe: replikacja „A+”. |
| APP-09 | Trzy klasy ustawień zamiast podziału host/klient. |
| MCP-01..05 | Nowe: MCP na poziomie wątków. |
| AC-02, 04, 07, 09, 11, 14, 24, 32 | Kroki i PASS dostosowane do powyższych zmian. |
| AC-36..40 | Nowe przypadki odbioru; macierz F-02, F-15 i F-16 odwołuje się do AC-37, AC-39 i AC-40. |

## Otwarte kwestie

1. Adresowanie zasobów bez wątku. Pliki, terminal, Git i Tasks projektu mogą istnieć
   bez wątku. `WorkspaceRef` znika z API. Trzeba ustalić klucz tych zasobów, np.
   `EnvironmentRef` z identyfikatorem projektu lub worktree.
2. Retencja i limity repliki. Pełny transkrypt i artefakty na każdym hoście wymagają
   limitu dysku, zasady wygaszania i zachowania przy przekroczeniu.
3. Lista klas ustawień. Każde obecne ustawienie (motyw, zoom, skróty, layout, nawigacja,
   narzędzia, profile, integracje) wymaga przypisania do klasy host, urządzenie
   albo preferencja użytkownika.
4. Nawigacja okien w stanie globalnym. F-16 i AC-24 wymagają niezależnej nawigacji
   okien. Trzeba ustalić, które okno zmienia akcja widoku z MCP, skoro MCP nie
   adresuje okien.
5. Zasięg jednej instancji. Trzeba ustalić, czy blokada działa per użytkownik systemu
   czy per katalog danych. Dotyczy to `CANOPY_DATA_DIR` i stanowiska z hostami A i B
   na jednym komputerze.
6. Quit przez MCP. LIFE-06 zabrania zdalnego Quit. Pełny parytet MCP z UI obejmuje
   Quit lokalnego hosta. Trzeba wskazać, czy Quit jest wyjątkiem od parytetu.
7. Zmiana `executor_epoch` w pierwszym etapie. Trzeba wymienić operacje hosta,
   które zwiększają epokę bez handoffu (np. odtworzenie po utracie danych).
8. Relacja z obecnym serwerem `canopy --chat-tools`. Trzeba ustalić, czy narzędzia
   stron HTML dla agentów czatu zostają osobnym serwerem MCP, czy stają się częścią
   rejestru komend.
9. MCP w macierzy odbioru. Trzeba ustalić, czy MCP jest osobną kolumną macierzy
   funkcji, czy wystarcza test kontraktowy i AC-38.
10. Konflikt rewizji przy automatycznym replay metadanych. Plan wymaga jawnego
    konfliktu. Trzeba ustalić prezentację i akcje użytkownika w UI.
11. Nazwy plików dokumentów. `agent-runtime-remote-*.md` zawierają słowo „remote”.
    Zmiana nazw wymaga aktualizacji odnośników w `AGENTS.md` i innych dokumentach.
