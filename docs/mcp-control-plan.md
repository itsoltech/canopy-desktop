# Sterowanie Canopy przez MCP

Status: projekt do uzgodnienia, bez implementacji. Data: 2026-10-31.

Ten plan opisuje serwer MCP (Model Context Protocol), przez który zewnętrzny agent AI
steruje aplikacją Canopy: projektami, worktree, rozmowami agentów, decyzjami,
terminalami, Git, PR, Tasks, plikami, ustawieniami i stanem hostów.
Plan rozszerza [plan domenowy agentów i remote](agent-runtime-remote-plan.md).
Nie zmienia jego wymagań. Gdzie ten dokument milczy, obowiązują SIMPLE, OWN, CMD,
CHAT, LIFE, APP, TERM, SEC i PERF z planu domenowego oraz
[kontrakty projektu](project-contracts.md).

`MUSI`, `NIE WOLNO` i `MOŻE` mają znaczenie z sekcji 1 planu domenowego.
Identyfikatory wymagań są stałe. Numeracja przypadków odbioru kontynuuje
[macierz odbioru](agent-runtime-remote-acceptance.md) od AC-36.

Wewnętrzny serwer stron (`canopy-desktop --chat-tools`, [`chat::mcp`](../src/chat/mcp.rs))
pozostaje osobny. Udostępnia agentowi rozmowy tylko `show_html` i `preview_html`.
Ten plan nie zmienia jego zakresu ani uprawnień.

## 1 Cele, zakres i non-goals

Cel produktu: agent AI uruchomiony poza Canopy albo w rozmowie Canopy może
obserwować i prowadzić pracę wielu agentów na wielu hostach. Może wydać polecenie,
odczytać wynik, zareagować na decyzję i przygotować zmiany Git. Człowiek zachowuje
kontrolę nad skutkami o wysokim ryzyku.

| ID | Wymaganie |
| --- | --- |
| MCP-01 | Serwer MCP MUSI być trzecim adapterem tych samych handlerów domenowych co lokalny desktop i gRPC (SIMPLE-05, RPC-01). NIE WOLNO dodawać drugiej implementacji operacji Git, plików, czatu, terminala ani Tasks. |
| MCP-02 | Każde narzędzie MCP MUSI odpowiadać istniejącej operacji Canopy z sekcji 3 planu domenowego (F-01..F-17). Nowa operacja biznesowa wymaga najpierw handlera dostępnego także dla UI i gRPC. |
| MCP-03 | Serwer MCP MUSI działać tylko w działającej aplikacji Canopy (SCOPE-02, SCOPE-04). Brak aplikacji daje klientowi jawny błąd połączenia. Nie powstaje daemon ani tryb headless. |
| MCP-04 | Sterowanie MCP MUSI być domyślnie wyłączone. Włączenie jest jawną decyzją użytkownika hosta w Preferences → Connections → Automation. |
| MCP-05 | Klient MCP MUSI być osobnym principalem z własnymi zakresami, rootami, limitami i audytem. NIE WOLNO mu dziedziczyć principala właściciela hosta ani credentialu innego urządzenia. |
| MCP-06 | Model rozmowy, kolejka, decyzje, cleanup i trwałość MUSZĄ zachować reguły planu domenowego. MCP nie dodaje skrótów omijających preflight, guardy usuwania, kolejność kolejki ani zgody backendu. |

Non-goals tego planu:

- Odczyt lub eksport sekretów, tokenów kont, kluczy TLS i SSH. Zapis sekretu przez MCP
  jest również poza zakresem (sekcja 4.4).
- Parowanie urządzeń, zmiana uprawnień, revoke, konfiguracja Remote i Quit hosta przez MCP.
- Edycja AgentDefinition, profili i argv/env procesów. Te ustawienia wykonują kod na hoście.
- Sterowanie widokiem: fokus, taby, układ okien, scroll i motyw należą do klienta (OWN-03, APP-09).
- Publiczny endpoint MCP w sieci, przekaźnik w chmurze, konto online i OAuth z zewnętrznym
  serwerem autoryzacji. Remote działa przez istniejący gRPC (sekcja 5).
- Ogólny endpoint `execute(method, json)` lub przekazywanie surowych komend Protobuf.
- Nowe funkcje biznesowe, np. automatyczne scalanie PR, harmonogramy zadań lub reguły
  automatyzacji po stronie hosta. Agent MCP może je realizować sam, przez istniejące operacje.

## 2 Architektura

### 2.1 Miejsce adaptera

```text
 klient MCP (stdio)            klient MCP (HTTP loopback)      agent rozmowy Canopy
        │                               │                              │
 canopy-desktop --mcp ── gniazdo ──┐    │                 canopy-desktop --mcp (token czatu)
                                   ▼    ▼                              │
                     ┌──────── McpGateway (w aplikacji) ◄──────────────┘
                     │  principal, zakresy, potwierdzenia, audyt, limity
                     │  mapowanie JSON ⇄ typy żądań usług
                     ├──► usługi hosta lokalnego (ChatsRpc, CatalogRpc, GitRpc, FilesRpc,
                     │    TerminalRpc, TasksRpc, SettingsRpc, HostRpc) ─► właściciele stanu
                     └──► ClientRegistry/HostClient ─► gRPC mTLS ─► host B, host C …
```

McpGateway jest obiektem aplikacji, obok `RunningServer` i `ClientRegistry`.
Nie ma dostępu do `Window`. Nie mutuje encji GPUI z obcego wątku. Wywołuje
typowane usługi, które już przekazują polecenia do właścicieli stanu
(`ControlBridge`, runtime czatów, workery Git i plików).

### 2.2 Ścieżka wywołania handlerów

Porównanie według SIMPLE-02:

| Wariant | Ocena |
| --- | --- |
| A. Gateway jako klient gRPC przez loopback TLS z własnym certyfikatem | Pełne ponowne użycie auth, inboxu i revoke. Wymaga włączonego listenera (konflikt z SEC-09 przy wyłączonym Remote), kluczy klienta w procesie aplikacji i podwójnego kodowania Protobuf. |
| B. Gateway wywołuje te same obiekty usług w procesie | Te same handlery, inbox, dedupe, blokady zasobów i strumienie. Bez sieci i bez listenera. Wymaga przekazania principala inaczej niż przez certyfikat TLS. |
| C. Nowy command bus dla UI, gRPC i MCP | Duplikuje istniejący podział na typowane usługi. Plan domenowy go nie wymaga. Odrzucony. |

Wybór: wariant B. Gateway trzyma klony tych samych obiektów usług, które
`ServerServices.features` rejestruje w tonic. Wywołuje metody traitów usług
z `tonic::Request` utworzonym w procesie. Strumienie (`SubscribeChat`, `ReadFile`,
`ReadDiff`, `WatchSummaries`) gateway konsumuje lokalnie i ogranicza ich rozmiar.

| ID | Wymaganie |
| --- | --- |
| MGW-01 | `RpcAuth::authenticate` MUSI przyjąć principala MCP tylko z rozszerzenia żądania wstawionego przez gateway w procesie. Typ rozszerzenia NIE MOŻE być możliwy do utworzenia z danych sieciowych. Ścieżka certyfikatu TLS pozostaje bez zmian. |
| MGW-02 | Obiekty usług MUSZĄ istnieć niezależnie od włączenia listenera Remote. Wyłączony Remote access nie wyłącza MCP i odwrotnie. |
| MGW-03 | Każda mutacja MCP MUSI przejść przez inbox z `(HostId, DeviceId principala, command_id)` oraz command_epoch (CMD-01..CMD-12). Gateway jest właścicielem epoki i outboxu principala, tak jak klient urządzenia. |
| MGW-04 | Gateway NIE MOŻE trzymać blokady GPUI ani stanu aplikacji podczas oczekiwania na wynik. Oczekiwanie na turę agenta nie blokuje innych wywołań MCP ani UI. |
| MGW-05 | Typy wejścia i wyjścia MCP MUSZĄ być odwzorowane z typów żądań usług przez jawne funkcje mapujące. NIE WOLNO udostępniać pól Protobuf, które nie mają znaczenia dla danej operacji MCP (np. ręczne command_epoch). |

Rozszerzenie żądania jest wąską zmianą istniejącego kodu. Nie wymaga nowej warstwy
autoryzacji. Kontrola zakresów (`RpcAuth::scope`, `authorize_runtime_read`) działa
tak samo dla urządzenia i dla principala MCP, bo oba mają `DeviceGrant`.

## 3 Transport

### 3.1 Protokół

Bazowa wersja protokołu to MCP `2025-06-18`. Gateway negocjuje wersję w `initialize`.
Obsługa nowszej rewizji wymaga sprawdzenia jej zmian i tych samych testów kontraktowych.
Raport odbioru zapisuje wersje protokołu i przetestowanych klientów.

Serwer ogłasza capabilities `tools` (z `listChanged`), `resources` (z `subscribe`
i `listChanged`) oraz `logging`. Nie ogłasza `prompts`, `sampling` ani `completions`.
Gateway nie wysyła żądań `elicitation` do klienta (sekcja 4.3).

### 3.2 Stdio przez most procesu

Klient MCP uruchamia `canopy-desktop --mcp [--client <nazwa>]`. Proces mostu nie
otwiera bazy, nie ładuje GPUI i nie wykonuje operacji. Łączy stdin/stdout z gniazdem
sterowania aplikacji i przekazuje ramki JSON-RPC. Kod ramek i limit linii
pochodzi z istniejącego `chat::mcp` (jedna wiadomość na linię, limit 4 MiB).

| ID | Wymaganie |
| --- | --- |
| MTR-01 | Gniazdo sterowania MUSI być gniazdem Unix 0700 w katalogu danych Canopy albo named pipe Windows z ACL tylko dla bieżącego użytkownika. Ścieżka nie zależy od `/tmp` wspólnego dla użytkowników. |
| MTR-02 | Most MUSI zakończyć się jawnym błędem JSON-RPC, gdy aplikacja nie działa, gdy MCP jest wyłączone albo gdy host jest w draining. NIE WOLNO uruchamiać aplikacji ani czekać bez limitu. |
| MTR-03 | Zamknięcie stdin MUSI zamknąć sesję MCP i jej subskrypcje oraz zwolnić lease terminala. NIE MOŻE przerwać tury, odrzucić decyzji ani usunąć zasobu (LIFE-02). |
| MTR-04 | Most MUSI uwierzytelnić się credentialem principala (sekcja 4.2). Dostęp do gniazda nie jest uprawnieniem. |

### 3.3 Streamable HTTP na loopback

Streamable HTTP jest drugą drogą dla klientów bez stdio. Jest wyłączony, dopóki
użytkownik go nie włączy w ustawieniach Automation.

| ID | Wymaganie |
| --- | --- |
| MTR-05 | Endpoint HTTP MUSI nasłuchiwać wyłącznie na `127.0.0.1` i `::1`. NIE WOLNO ustawić adresu LAN, VPN ani `0.0.0.0`. Dostęp sieciowy realizuje sekcja 5. |
| MTR-06 | Endpoint MUSI sprawdzać nagłówki `Origin` i `Host` przeciw DNS rebinding. Żądanie przeglądarkowe z obcego origin dostaje 403 przed odczytem body. |
| MTR-07 | Endpoint MUSI wymagać `Authorization: Bearer` z tokenem principala. `Mcp-Session-Id` jest losowym identyfikatorem sesji, a nie credentialem. Brak lub zły token daje 401 bez szczegółów zasobów (SEC-03). |
| MTR-08 | Strumień SSE MUSI mieć limit kolejki wyjścia. Wolny klient dostaje utratę powiadomień zamiast blokady gatewaya. Odczyty narzędzi pozostają poprawne, bo powiadomienia są tylko wskaźnikami zmian (sekcja 8). |

Implementacja HTTP MOŻE używać `hyper`/`axum`, które są już w Cargo.lock jako
zależności tonic. Bezpośrednia zależność wymaga uzasadnienia SIMPLE-01.

### 3.4 Lokalny i zdalny host

MCP zawsze łączy się z lokalną aplikacją Canopy. Sterowanie innym komputerem
przechodzi przez gRPC tej aplikacji do zaufanego hosta (sekcja 5). Host B nie
otwiera endpointu MCP dla sieci. Dzięki temu remote ma jeden model zaufania:
pairing, mTLS, zakresy, revoke i inbox z planu domenowego.

## 4 Principal, parowanie, uprawnienia i audyt

### 4.1 Rodzaje principala

| Principal | Pochodzenie | Trwałość |
| --- | --- | --- |
| Klient automatyzacji | Zatwierdzony przez użytkownika hosta most stdio lub token HTTP. | Wiersz zaufanego urządzenia z `kind = automation`. |
| Agent rozmowy | Proces czatu Canopy z włączoną opcją Canopy control w profilu lub rozmowie. | Ulotny, związany z run czatu. Znika przy końcu runu. |

Principal automatyzacji korzysta z tabeli zaufanych urządzeń. Ma DeviceId,
zakresy, rooty, `authorization_revision`, command_epoch i stan revoked.
Nie ma certyfikatu TLS. Credentialem jest losowy token.

| ID | Wymaganie |
| --- | --- |
| MAU-01 | Token principala MUSI mieć minimum 256 bitów z systemowego generatora. Host przechowuje tylko MAC tokenu z kluczem z magazynu systemowego. Tokenu nie wolno zapisać w logu, inboxie ani diagnostyce (SEC-06). |
| MAU-02 | Token agenta rozmowy MUSI być ważny tylko dla jednego runu i zostać unieważniony przy jego końcu, Restart, Delete i Quit, tak jak token usługi stron. |
| MAU-03 | Principal automatyzacji MUSI wliczać się do limitu 32 ważnych tożsamości i do limitów inboxu per urządzenie (CMD-11). |
| MAU-04 | Revoke i zmiana zakresu MUSZĄ działać w ciągu 1 s także na otwartych sesjach MCP (SEC-04): zamykają subskrypcje, zwalniają lease terminala, anulują oczekujące potwierdzenia i wysyłają `notifications/tools/list_changed`. |

### 4.2 Parowanie klienta

Parowanie stdio:

1. Użytkownik dodaje do konfiguracji klienta MCP polecenie `canopy-desktop --mcp --client <nazwa>`.
2. Most bez credentialu wysyła żądanie parowania przez gniazdo. Host pokazuje w notchu
   i w Preferences prośbę z nazwą klienta, ścieżką procesu nadrzędnego, jeśli jest znana,
   oraz proponowanymi zakresami.
3. Użytkownik hosta wybiera zakresy, rooty, hosty zdalne i politykę potwierdzeń.
   Domyślna propozycja to `read` i `chat` dla wszystkich projektów; `approve` jest odznaczone.
4. Host zapisuje principala, generuje token i oddaje go mostowi raz. Most zapisuje
   token w magazynie systemowym bieżącego użytkownika pod kluczem nazwy klienta.
5. Okno prośby wygasa po 5 minutach. Odmowa lub wygaśnięcie kończy most błędem.

Parowanie HTTP: Preferences → Connections → Automation → Add client tworzy principala
z wybranymi zakresami i pokazuje token raz, razem z przykładową konfiguracją klienta.
Utrata tokenu wymaga nowego tokenu; host nie pokazuje starego.

| ID | Wymaganie |
| --- | --- |
| MAU-05 | Prośba parowania NIE MOŻE dawać dostępu do danych przed zatwierdzeniem (SEC-03). Nazwa klienta i ścieżka procesu są tylko etykietą, nie dowodem tożsamości. |
| MAU-06 | Jednoczesne prośby MUSZĄ być widoczne osobno. Jedna zgoda zatwierdza dokładnie jedną prośbę. Maksymalnie 4 oczekujące prośby; kolejne dostają `rate_limited`. |
| MAU-07 | Lista principali MUSI pokazywać nazwę, rodzaj, zakresy, rooty, ostatnie użycie i aktywne sesje. Revoke jednego principala nie wpływa na urządzenia i innych klientów. |

### 4.3 Zakresy i potwierdzenia na hoście

Principal MCP używa istniejących zakresów z `src/host.rs`:
`read`, `chat`, `approve`, `files_git`, `terminal`, `tasks` oraz rootów projektów.
`admin` NIE jest nadawany principalowi MCP. Operacje admin są poza zakresem MCP.

Każde narzędzie ma klasę ryzyka:

| Klasa | Przykłady | Domyślna polityka |
| --- | --- | --- |
| R0 odczyt | listy, status, historia, diff, odczyt pliku, ekran terminala | Bez potwierdzenia, w ramach `read` i rootów. |
| R1 mutacja odwracalna w zakresie | utworzenie czatu, Send, kolejka, Interrupt, stage/unstage, zapis pliku, utworzenie terminala | Bez potwierdzenia, w ramach zakresu. |
| R2 skutek zewnętrzny lub trudny do cofnięcia | odpowiedź na decyzję agenta, commit, Push, Pull, utworzenie PR, zmiana zadania, task do worktree, utworzenie worktree, wejście do terminala utworzonego przez kogoś innego, zmiana preferencji hosta | Potwierdzenie na hoście, które użytkownik może zmienić na „bez potwierdzenia” per principal i narzędzie. |
| R3 destrukcyjne | discard, usunięcie czatu/terminala, zamknięcie projektu, merge/remove/stop worktree | Zawsze potwierdzenie na hoście z planem skutków z preflight. Nie można go wyłączyć. |
| Zabronione | sekrety, urządzenia, Remote, Quit, AgentDefinition, profile, zakresy MCP | Narzędzie nie istnieje. |

Potwierdzenie pojawia się w Canopy na hoście wykonującym gateway: w notchu,
w liście decyzji i jako toast. Zawiera principala, host docelowy, zasób,
streszczenie argumentów oraz skutki z preflight. Akcje: Allow once, Deny.
Dla R2 jest też Allow for this session (sesja MCP, nie principal).

| ID | Wymaganie |
| --- | --- |
| MCF-01 | Potwierdzenie MUSI odbywać się w zaufanym UI Canopy. NIE WOLNO używać MCP elicitation ani treści od klienta MCP jako dowodu zgody człowieka. |
| MCF-02 | Gateway MUSI zapytać o potwierdzenie przed zapisem accepted w inboxie. Oczekujące potwierdzenie jest stanem w pamięci gatewaya. Restart, revoke, koniec sesji MCP lub 5 minut bez odpowiedzi daje `confirmation_expired` bez skutku. |
| MCF-03 | Potwierdzenie MUSI być związane z fingerprintem operacji (metoda, cel, argumenty, plan). Zmiana argumentów wymaga nowego potwierdzenia. Plan z preflight jest ponownie sprawdzany przed skutkiem (LIFE-03). |
| MCF-04 | Wywołanie wymagające potwierdzenia MUSI zwrócić wynik od razu ze stanem `awaiting_confirmation` i `confirmation_id`. Wynik końcowy klient pobiera przez `operation_get` albo powiadomienie zasobu operacji. Tool call nie czeka 5 minut. |
| MCF-05 | Agent rozmowy NIE MOŻE odpowiadać na decyzje własnej rozmowy ani zatwierdzać potwierdzeń MCP. Odpowiedź na decyzję innej rozmowy wymaga `approve` i podlega MCF-01..04. |
| MCF-06 | Polityka „bez potwierdzenia” dla R2 MUSI być jawnie ustawiona przez użytkownika hosta dla konkretnego principala i narzędzia. Nie wynika z `approve` ani z domyślnej zgody `*`. |

Domyślna zgoda urządzeń remote (`*` dla wszystkich zakresów i zasobów) nie dotyczy
principali MCP. Klient automatyzacji dostaje tylko zakresy zaznaczone przy parowaniu.

### 4.4 Sekrety

Narzędzia MCP nie zwracają wartości env profili, tokenów integracji ani credentiali.
Snapshot ustawień ma te same reguły co `SettingsService.GetSettings` (SEC-06).
Zapis sekretu wymaga admin (APP-10), więc MCP go nie obsługuje. Uprawniony odczyt
pliku, transkryptu lub terminala może zawierać sekret wpisany przez użytkownika;
plan domenowy nie obiecuje filtrowania dowolnej treści i ten plan również nie.

### 4.5 Audyt

Mutacje principala MCP są zapisane w inboxie: DeviceId, command_id, metoda, cel,
stan i czasy. Inbox nie wystarcza dla odczytów, odmów i potwierdzeń.

| ID | Wymaganie |
| --- | --- |
| MAU-08 | Host MUSI prowadzić ograniczony dziennik audytu MCP: czas, principal, sesja, host docelowy, narzędzie, cel, klasa ryzyka, decyzja potwierdzenia, wynik i command_id. Dziennik NIE zawiera promptów, treści plików, wyjścia terminala ani argumentów tekstowych (PERF-03). |
| MAU-09 | Dziennik MUSI mieć limit 50 000 wpisów i 30 dni per host, z usuwaniem najstarszych. Pełny dziennik nie blokuje operacji. |
| MAU-10 | Preferences MUSI pokazywać aktywność principala i pozwalać na eksport dziennika bez sekretów. Notch MUSI pokazywać wskaźnik aktywnej sesji MCP i akcję „Pause automation”, która od razu odrzuca nowe mutacje wszystkich principali MCP. |

Pause automation nie przerywa już przyjętych tur ani poleceń (SEC-05). Zatrzymanie
pracy agenta pozostaje osobną akcją użytkownika.

## 5 Wiele hostów

Każde narzędzie przyjmuje `host_id`. Gdy pole jest puste, celem jest host lokalny
gatewaya. NIE jest nim host aktualnie oglądany w oknie (OWN-04).

`hosts_list` zwraca host lokalny i hosty z `ClientRegistry`: HostId, nazwę,
stan połączenia, last_seen, readiness, wersję kontraktu oraz efektywne zakresy MCP.

Operacja na hoście B przechodzi przez `HostClient` aplikacji A, z credentialem
urządzenia A na hoście B. Efektywne uprawnienie to część wspólna:

1. zakresów i rootów principala MCP na A, w tym listy dozwolonych hostów zdalnych;
2. zakresów i rootów, które host B nadał urządzeniu A.

| ID | Wymaganie |
| --- | --- |
| MHOST-01 | Principal MCP MUSI mieć jawną listę hostów zdalnych. Pusta lista oznacza tylko host lokalny. Nowo sparowany host nie trafia na listę automatycznie. |
| MHOST-02 | Komenda MCP do hosta B MUSI mieć `command_id` principala i command_epoch urządzenia A na B. Gateway zapisuje powiązanie command_id z principalem lokalnie, aby `operation_get` zwracał wynik tylko właściwemu principalowi. |
| MHOST-03 | Każda mutacja MCP na hoście zdalnym MUSI oznaczać pochodzenie `automation` i nazwę principala w metadanych komendy. Host B zapisuje je w inboxie i audycie jako informację; uprawnienie nadal wynika z credentialu A. |
| MHOST-04 | Host B MUSI móc wyłączyć przyjmowanie komend o pochodzeniu `automation` od danego urządzenia. Odmowa daje `permission_denied` z powodem `automation_disabled`. |
| MHOST-05 | Niedostępny host MUSI dawać `host_unavailable` z last_seen. Nie blokuje wywołań do innych hostów (OWN-05). Gateway NIE ponawia automatycznie mutacji po zmianie trasy; stosuje CMD-12 z tym samym command_id. |
| MHOST-06 | Potwierdzenie R2/R3 dla hosta B MUSI pojawić się na hoście A, który wykonuje gateway. Host B stosuje dodatkowo własne guardy i preflight. |

MHOST-03 i MHOST-04 rozszerzają `CommandMeta` o pole addytywne. Host B nie może
zweryfikować kryptograficznie, że komenda pochodzi od automatyzacji; ufa urządzeniu A
w granicach jego zakresów, tak jak dziś. Pole służy do audytu i polityki właściciela.

## 6 Konwencje narzędzi

### 6.1 Nazwy, opis i adnotacje

Nazwy narzędzi mają postać `domena_czynność` w snake_case, np. `chat_send`.
Każde narzędzie ma `title`, `description`, `inputSchema` i `outputSchema`.
Adnotacje: `readOnlyHint` dla R0, `destructiveHint` dla R3, `idempotentHint`
dla operacji z `request_id`, `openWorldHint` dla Git network, PR i Tasks.
Adnotacje są wskazówką dla klienta. Host nie opiera na nich autoryzacji.

`tools/list` zwraca tylko narzędzia dostępne dla zakresów principala i bieżącej fazy.
Ukrycie narzędzia nie zastępuje kontroli przy wywołaniu (SEC-08).

### 6.2 Typy wspólne

Notacja `{pole: typ, opcja?: typ}` oznacza JSON Schema `type: object`
z `additionalProperties: false`. Pole z `?` jest opcjonalne.

| Typ | Kształt |
| --- | --- |
| `HostId` | `string` UUID; puste lub brak oznacza host lokalny gatewaya. |
| `WorkspaceRef` | `{host_id?: HostId, workspace_id: string}` |
| `ChatRef` | `{host_id?: HostId, chat_id: string}` |
| `TerminalRef` | `{host_id?: HostId, terminal_id: string}` |
| `FileRef` | `{workspace: WorkspaceRef, path: string}`; `path` względny do rootu workspace, separator `/`. |
| `ItemKey` | `string` nieprzezroczysty; koduje `(history_epoch, item)`. |
| `Cursor` | `string` nieprzezroczysty; koduje `runtime_epoch`, `stream_id`, `sequence`. |
| `PageToken` | `string` nieprzezroczysty; unieważniany według SYNC-05. |
| `RequestId` | `string` UUID; staje się `command_id`. |
| `Receipt` | `{host_id, command_id, state: "awaiting_confirmation"\|"accepted"\|"attempting"\|"succeeded"\|"failed"\|"rejected"\|"delivery_unknown"\|"command_retired", confirmation_id?, resource_id?, message_id?, resolved_by_user: bool, error?: Error}` |
| `Error` | `{code: string, message: string, retryable: bool, host_id?, command_id?, details?: object}` (sekcja 9) |

### 6.3 Idempotencja i długie operacje

Każde narzędzie mutujące przyjmuje opcjonalne `request_id`. Gateway generuje UUID,
gdy go brak, i zawsze zwraca `command_id` w `Receipt`.

| ID | Wymaganie |
| --- | --- |
| MTL-01 | Powtórzenie wywołania z tym samym `request_id` i tymi samymi argumentami MUSI zwrócić stan pierwszej komendy (CMD-02). Inne argumenty dają `idempotency_conflict`. |
| MTL-02 | Gateway MUSI zamykać epokę principala dopiero, gdy nie ma accepted ani attempting, a wyniki zostały oddane klientowi lub przekroczyły 24 h. Retry po zamknięciu daje `command_retired`, nigdy nowy skutek (CMD-10). |
| MTL-03 | Narzędzie NIE MOŻE czekać dłużej niż 60 s. Operacja dłuższa zwraca `Receipt`, a klient sprawdza wynik przez `operation_get`, `chat_wait` lub subskrypcję. |
| MTL-04 | Gateway MUSI obsługiwać `notifications/cancelled`. Anulowanie przerywa oczekiwanie i odczyt. NIE anuluje przyjętej mutacji (RPC-03). Jawne anulowanie jest osobnym narzędziem tylko dla operacji, które je wspierają. |
| MTL-05 | Wywołanie z `_meta.progressToken` MUSI dostawać `notifications/progress` dla Pull, Push, preflight i `chat_wait`. Brak tokenu nie zmienia wyniku. |
| MTL-06 | Delivery_unknown MUSI być zwracane jako stan, nie jako sukces ani zwykły błąd. Rozstrzygnięcie niepewnego wyniku pozostaje decyzją użytkownika w UI (CMD-06). |

### 6.4 Limity

| Zasób | Limit |
| --- | --- |
| Wynik narzędzia | 1 MiB po serializacji (limit control). Większe dane wymagają stronicowania. |
| Tekst w jednym elemencie odpowiedzi | Domyślnie 16 KiB, `max_chars` do 256 KiB; obcięcie jest jawne (`truncated: true`). |
| Odczyt pliku | Do 256 KiB na wywołanie z `offset`; pliki binarne dają metadane i `binary: true`. |
| Strona historii | Do 100 elementów (limit planu domenowego). |
| Równoległe wywołania | 8 na sesję MCP, 4 równoczesne `chat_wait`. |
| Mutacje | 10/s z burstem 20 na principal; `chat_create` 10/min; `chat_send` 30/min. Wartości są początkowe i podlegają pomiarowi. |
| Sesje | 4 sesje na principal, 16 na host. |

Przekroczenie limitu daje `rate_limited` z `retry_after_ms` albo `resource_exhausted`
z możliwością odzyskania (PERF-01). Rozmowy uruchomione przez MCP wliczają się
do limitu 32 procesów czatu (LIFE-09).

## 7 Katalog narzędzi

Kolumna Faza: M1 = MVP, M2 i M3 według sekcji 12. Kolumna Usługa wskazuje istniejącą
metodę gRPC. „Nowy handler” oznacza lukę, którą trzeba zamknąć wspólnym handlerem
dla UI, gRPC i MCP przed udostępnieniem narzędzia.

### 7.1 Hosty, status i operacje

| Narzędzie | Klasa, zakres | Faza | Usługa | Wejście → wyjście |
| --- | --- | --- | --- | --- |
| `hosts_list` | R0 read | M1 | HostService.GetInfo, ClientRegistry | `{}` → `{hosts: [{host_id, name, local: bool, connection: "online"\|"offline"\|"connecting", readiness, last_seen?, protocol_version, scopes: [string]}]}` |
| `status_overview` | R0 read | M1 | CatalogService.ReadCatalog | `{host_ids?: [HostId]}` → `{hosts: [{host_id, readiness, chats: {working, needs_decision, idle, failed}, pending_decisions: [DecisionSummary], stale: bool}]}` |
| `operation_get` | R0 read | M1 | HostService.GetCommand | `{host_id?, command_id}` → `Receipt` |
| `operations_list` | R0 read | M2 | inbox principala | `{host_id?, state?: string, page_token?}` → `{operations: [Receipt & {method, target}], next_page_token?}` |
| `agents_list` | R0 read | M1 | CatalogService.ListAgents, GetAgentCatalog | `{host_id?}` → `{agents: [{agent_id, name, backend, profiles: [{profile_id, name}], models: [string]}]}` |

`DecisionSummary` = `{chat: ChatRef, chat_title, decision_id, run_id, title, kind?}`.

### 7.2 Projekty i worktree

| Narzędzie | Klasa, zakres | Faza | Usługa | Wejście → wyjście |
| --- | --- | --- | --- | --- |
| `projects_list` | R0 read | M1 | CatalogService.ListResources | `{host_id?}` → `{projects: [{project_id, name, path, workspaces: [{workspace: WorkspaceRef, name, path, branch?, is_worktree: bool, closing: bool}]}]}` |
| `project_open` | R1 chat | M2 | CatalogService.OpenProject | `{host_id?, path, request_id?}` → `Receipt` |
| `worktree_preflight` | R0 read | M2 | GitService.Preflight (worktree) | `{workspace, action: "create"\|"merge"\|"remove"\|"stop", branch?, start_point?, target_branch?, delete_branch?, discard_changes?}` → `GitPlan` |
| `worktree_create` | R2 files_git | M2 | GitService.ChangeWorktree | `{workspace, branch, start_point?, path?, plan_id, request_id?}` → `Receipt` (`resource_id` = nowy workspace_id) |
| `worktree_merge` / `worktree_remove` / `worktree_stop` | R3 files_git | M2 | GitService.ChangeWorktree | `{workspace, plan_id, request_id?}` + pola akcji z preflight → `Receipt` |
| `close_preflight` | R0 read | M2 | CatalogService.PreflightClose | `{resource: {host_id?, kind: "chat"\|"terminal"\|"project", id}, delete: bool}` → `{plan_id, risks: [{kind, description}], can_execute, reason}` |
| `close_execute` | R3 chat/terminal | M2 | CatalogService.CloseResource | `{resource, plan_id, request_id?}` → `Receipt` |

`GitPlan` = `{plan_id, impacts: [string], can_execute: bool, reason, expires_at}`.
Plan ma te same warunki co UI. Wygasły lub nieaktualny plan daje `precondition_failed`.

### 7.3 Rozmowy agentów

| Narzędzie | Klasa, zakres | Faza | Usługa | Wejście → wyjście |
| --- | --- | --- | --- | --- |
| `chats_list` | R0 read | M1 | CatalogService.ReadCatalog | `{host_id?, workspace?: WorkspaceRef, private?: bool, state?: string}` → `{chats: [ChatSummary]}` |
| `chat_create` | R1 chat | M1 | CatalogService.CreateChat, ChatService.SetChatAgent | `{workspace?: WorkspaceRef, agent_id, profile_id, model?, title?, request_id?}` → `Receipt` (`resource_id` = chat_id) |
| `chat_send` | R1 chat | M1 | ChatService.SendMessage | schemat 7.9 |
| `chat_read` | R0 read | M1 | ChatService.ReadHistoryPage, projekcja SubscribeChat | `{chat, after?: ItemKey, page_token?, limit?: 1..100, max_chars?}` → `{items: [Item], next_page_token?, cursor, saved_boundary?: ItemKey}` |
| `chat_wait` | R0 read | M1 | ChatService.SubscribeChat | `{chat, cursor?, until: ["idle"\|"needs_decision"\|"turn_end"\|"any_change"], timeout_ms?: ≤60000}` → `{state, cursor, timed_out: bool, new_items: int, pending_decisions: [Decision]}` |
| `chat_search` | R0 read | M2 | ChatService.SearchHistory | `{chat, query, page_token?}` → `{matches: [{item: ItemKey, snippet}], next_page_token?, limited: bool}` |
| `chat_interrupt` | R1 chat | M1 | ChatService.Interrupt | `{chat, request_id?}` → `Receipt` |
| `chat_steer` | R1 chat | M2 | ChatService.Steer | `{chat, run_id, text, request_id?}` → `Receipt`; brak capability daje `unsupported` (CHAT-03). |
| `chat_queue` | R0 read | M1 | projekcja SubscribeChat | `{chat}` → `{revision, paused, entries: [{message_id, text, delivery}]}` |
| `chat_queue_change` | R1 chat | M2 | ChatService.ChangeQueue | `{chat, queue_revision, change: {edit: {message_id, text}} \| {move: {message_id, before?}} \| {remove: {message_id}} \| {pause: bool}, request_id?}` → `Receipt` |
| `chat_restart` | R1 chat | M2 | ChatService.Restart | `{chat, request_id?}` → `Receipt` |
| `chat_options` | R0 read | M2 | ChatService.GetAgentOptions | `{chat}` → `{options: [{id, label, current, choices: [{id, label}], effect}], capabilities: [{operation, supported, reason}]}` |
| `chat_set_option` | R1 chat | M2 | ChatService.SetAgentOption | `{chat, option_id, value_id, request_id?}` → `Receipt` |
| `chat_fork` | R1 chat | M3 | ChatService.ForkChat | `{chat, at: ItemKey, request_id?}` → `Receipt` |
| `chat_set_goal` | R1 chat | M3 | ChatService.SetGoal | `{chat, action: "set"\|"pause"\|"resume"\|"clear", objective?}` → `Receipt` |
| `chat_rename` | R1 chat | M2 | CatalogService.RenameResource | `{chat, title, request_id?}` → `Receipt` |

`ChatSummary` = `{chat: ChatRef, workspace?: WorkspaceRef, title, agent_id, profile_id?,
state: "dormant"|"starting"|"idle"|"working"|"needs_decision"|"interrupting"|"exited"|"failed",
pending_decisions: int, queued_messages: int, run_id?, goal?: {objective, state}, origin?: "automation"}`.

`Item` = `{key: ItemKey, kind: "user"|"assistant"|"thinking"|"tool"|"plan"|"notice"|"shell"|"turn_end",
text?, truncated: bool, saved: bool, tool?: {kind: "read"|"edit"|"execute"|"search"|"fetch"|"plan"|"subagent"|"other",
title, status, locations?: [string]}, turn_end?: {outcome, error?, usage?}}`.
Treść narzędzi i diffy są skracane do streszczenia; pełny diff daje `git_diff`.

| ID | Wymaganie |
| --- | --- |
| MTL-07 | `chat_create` MUSI wymagać jawnego `agent_id` i `profile_id` (F-04). Gateway NIE wybiera konta domyślnie. Rozmowa nie aktywuje taba ani okna na hoście (OWN-07). |
| MTL-08 | `chat_send` MUSI używać kolejki hosta i kontraktu Send (CHAT-01, CHAT-02). Sukces oznacza przyjęcie do kolejki, nie wykonanie tury. Wynik tury klient odczytuje przez `chat_wait` i `chat_read`. |
| MTL-09 | `chat_read` MUSI oznaczać elementy live i saved (SYNC-06) oraz skrócone i trimmed (SYNC-09). NIE WOLNO zwracać surowych danych wire backendu. |
| MTL-10 | `chat_wait` MUSI kończyć się przy pierwszym spełnionym warunku `until`, przy timeout albo przy `resync_required` z nowym kursorem. Timeout nie jest błędem. |
| MTL-11 | Rozmowa utworzona przez MCP MUSI mieć widoczne w UI pochodzenie `automation` z nazwą principala. Pochodzenie nie zmienia uprawnień użytkownika hosta do tej rozmowy. |

### 7.4 Decyzje agentów

| Narzędzie | Klasa, zakres | Faza | Usługa | Wejście → wyjście |
| --- | --- | --- | --- | --- |
| `decisions_list` | R0 read | M1 | CatalogService.ReadCatalog, SubscribeChat | `{host_ids?, chat?}` → `{decisions: [Decision & {chat: ChatRef}]}` |
| `decision_respond` | R2 approve | M1 | ChatService.RespondDecision | schemat 7.9 |

`Decision` = `{decision_id, run_id, title, description?, kind?, state: "pending"|"resolved",
resolution?, options: [{id, label, description?}], questions?: [{question, header,
options: [{id, label}], multiple: bool}], can_remember: bool}`.

Decyzja pozostaje jedna dla wszystkich klientów (CHAT-05). Pierwsza ważna odpowiedź
wygrywa; MCP dostaje `already_resolved` z aktualnym wynikiem. Odpowiedź
ze starym `run_id` daje `stale_run`. `allow_for_chat` jest dostępne tylko przy
`can_remember` i zachowuje zakres chat (CHAT-06).

### 7.5 Terminale

| Narzędzie | Klasa, zakres | Faza | Usługa | Wejście → wyjście |
| --- | --- | --- | --- | --- |
| `terminals_list` | R0 read | M1 | CatalogService.ListResources | `{host_id?, workspace?}` → `{terminals: [{terminal: TerminalRef, workspace, title, running: bool, input_holder?: string}]}` |
| `terminal_read` | R0 read | M1 | TerminalService.ReadHistory | `{terminal, scrollback_lines?: ≤2000, page_token?}` → `{run_id, rows: [string], cursor: {row, col}, alternate_screen: bool, scrollback: [string], scrollback_lost: bool, next_page_token?}` |
| `terminal_open` | R1 terminal | M2 | TerminalService.OpenTerminal | `{workspace, tool_id?, profile_id?, request_id?}` → `Receipt` (`resource_id` = terminal_id) |
| `terminal_write` | R1/R2 terminal | M2 | AcquireInput, TerminalSession | `{terminal, run_id, text?, keys?: [string], bracketed_paste?: bool}` → `{queued_bytes, lease: {token, expires_at}}` |
| `terminal_release` | R1 terminal | M2 | ReleaseInput | `{terminal}` → `{released: bool}` |
| `terminal_stop` / `terminal_restart` | R1 terminal | M2 | StopTerminal, RestartTerminal | `{terminal, request_id?}` → `Receipt` |

| ID | Wymaganie |
| --- | --- |
| MTL-12 | `terminal_write` MUSI przejąć lease tylko wtedy, gdy lease jest wolny. Terminal utworzony przez tego principala to R1. Wolny terminal utworzony przez innego principala to R2. Lease trzymany przez innego klienta daje `conflict` z jego nazwą; MCP NIE przejmuje go siłą (TERM-02). |
| MTL-13 | Gateway MUSI odnawiać lease co 5 s, dopóki sesja MCP trwa i od ostatniego zapisu nie minęło 60 s. Potem lease wraca zgodnie z TERM-04. Lokalny użytkownik może odzyskać lease w każdej chwili. |
| MTL-14 | Wynik `terminal_write` oznacza zapis do kolejki hosta (TERM-05). Gateway NIE ponawia bajtów po utracie odpowiedzi. Terminal ACP pozostaje tylko do odczytu (AGT-10). |
| MTL-15 | `terminal_read` MUSI zwracać tekst projekcji ekranu hosta, a nie surowe bajty PTY. Sekwencje sterujące są usunięte; szerokie znaki zachowane. |

### 7.6 Git i pull requesty

| Narzędzie | Klasa, zakres | Faza | Usługa | Wejście → wyjście |
| --- | --- | --- | --- | --- |
| `git_status` | R0 read | M1 | GitService.GetStatus | `{workspace}` → `{branch?, upstream?, ahead, behind, head, staged: [FileChange], unstaged: [FileChange], conflicted: [string]}` |
| `git_diff` | R0 read | M1 | GitService.ReadDiff | `{workspace, path?, staged?: bool, commit?: string, max_bytes?: ≤1MiB}` → `{diff: string (unified), truncated: bool}` |
| `git_log` | R0 read | M2 | GitService.ReadHistory | `{workspace, ref?, page_token?, limit?: ≤100}` → `{commits: [{id, summary, author, time}], next_page_token?}` |
| `git_refs` | R0 read | M2 | GitService.ListRefs | `{workspace}` → `{branches: [{name, upstream?, head: bool}], remotes: [string], tags: [string]}` |
| `git_preflight` | R0 read | M2 | GitService.Preflight | `{workspace, intent: {paths: {operation, paths}} \| {commit: {message, sign}} \| {network: {action, remote, upstream_branch?}}}` → `GitPlan` |
| `git_stage` / `git_unstage` | R1 files_git | M2 | GitService.Stage, Unstage | `{workspace, paths: [string], plan_id, request_id?}` → `Receipt` |
| `git_discard` | R3 files_git | M2 | GitService.Discard | `{workspace, paths, plan_id, request_id?}` → `Receipt` |
| `git_commit` | R2 files_git | M2 | GitService.Commit | `{workspace, message, sign?: bool, plan_id, request_id?}` → `Receipt` (`resource_id` = commit id) |
| `git_pull` / `git_push` | R2 files_git | M2 | GitService.Pull, Push | `{workspace, remote, upstream_branch?, plan_id, request_id?}` → `Receipt` |
| `prs_list` | R0 read | M2 | CatalogService.ListPullRequests | `{workspace, query?, page_token?}` → `{pulls: [{number, title, state, branch, url}], next_page_token?}` |
| `pr_create` | R2 files_git, tasks | M3 | nowy handler | `{workspace, title, body?, base?, draft?: bool, request_id?}` → `Receipt` (`resource_id` = URL PR) |

`FileChange` = `{path, status: "added"|"modified"|"deleted"|"renamed"|"typechange", old_path?}`.

| ID | Wymaganie |
| --- | --- |
| MTL-16 | Każda mutacja Git MUSI wymagać aktualnego `plan_id` z preflight. Host ponownie sprawdza HEAD, indeks i stan worktree przed skutkiem (APP-06, APP-07). |
| MTL-17 | Commit MUSI uruchamiać hooki i podpis zgodnie z obecnymi zasadami. Prompt pinentry lub Keychain daje `needs_host_interaction` (APP-11). Retry niepewnego commita nie tworzy drugiego commita. |
| MTL-18 | `pr_create` MUSI użyć tego samego handlera co dialog PR w UI: Push gałęzi, potem utworzenie PR przez konto GitHub hosta. Ten handler trzeba wydzielić z `PullRequestsState` i udostępnić również w gRPC, zanim MCP go użyje. |

### 7.7 Tasks

| Narzędzie | Klasa, zakres | Faza | Usługa | Wejście → wyjście |
| --- | --- | --- | --- | --- |
| `tasks_integrations` | R0 read | M2 | TasksService.ListIntegrations | `{host_id?}` → `{integrations: [{integration_id, kind: "github"\|"jira"\|"youtrack", name}]}` |
| `tasks_query` | R0 read | M2 | TasksService.QueryTasks | `{host_id?, integration_id, project_id?, query?, filter_id?, state?, page_token?}` → `{tasks: [TaskSummary], next_page_token?}` |
| `task_get` | R0 read | M2 | TasksService.GetTask, ReadComments | `{task: TaskRef, comments?: bool}` → `{task: Task, comments?: [Comment]}` |
| `task_update` | R2 tasks | M3 | TasksService.UpdateTask, ApplyMutation | `{task, changes: {field_id: value}, expected_revision?, request_id?}` → `Receipt` |
| `task_to_worktree` | R2 tasks, files_git | M3 | TasksService.TaskToWorktree | `{task, workspace, branch, request_id?}` → `Receipt` |

Tasks używają kont hosta (APP-08). Token konta nie trafia do klienta MCP.
Schemat pól edycji pochodzi z `TasksService.GetSchema`; MCP nie zgaduje pól dostawcy.

### 7.8 Pliki i ustawienia

| Narzędzie | Klasa, zakres | Faza | Usługa | Wejście → wyjście |
| --- | --- | --- | --- | --- |
| `files_list` | R0 read | M1 | FilesService.ListDirectory | `{workspace, path?: string, page_token?}` → `{entries: [{path, kind: "file"\|"dir"\|"symlink", size?}], next_page_token?}` |
| `files_search` | R0 read | M1 | FilesService.SearchFiles | `{workspace, query, limit?: ≤200}` → `{entries: [{path, kind}]}` |
| `file_read` | R0 read | M1 | FilesService.ReadFile | `{file: FileRef, offset?, max_bytes?: ≤256KiB}` → `{text?, binary: bool, size, hash, eof: bool, encoding: {bom, line_ending}}` |
| `file_write` | R1 files_git | M2 | FilesService.SaveFile, CreateFile, Artifacts | `{file, text, expected_hash?: string, create?: bool, request_id?}` → `Receipt` + `file_hash` |
| `settings_get` | R0 read | M3 | nowy handler (projekcja GetSettings) | `{host_id?}` → `{revision, preferences: [{key, value}]}`: tylko klucze z listy dozwolonej (sekcja 7.10). |
| `settings_update` | R2, zakres do decyzji | M3 | nowy handler (podzbiór UpdatePreferences) | `{host_id?, changes: [{key, value}], expected_revision}` → `Receipt`. Tylko klucze z listy dozwolonej (sekcja 7.10). |

| ID | Wymaganie |
| --- | --- |
| MTL-19 | `file_write` MUSI wymagać `expected_hash` dla istniejącego pliku i stosować SaveFile: konflikt, dirty presence innego urządzenia, BOM, CRLF i uprawnienia (APP-02..APP-04). Brak `expected_hash` dla istniejącego pliku daje `precondition_failed`. NIE WOLNO dodać force overwrite. |
| MTL-20 | Ścieżki plików MUSZĄ być względne do rootu workspace i sprawdzane po kanonizacji oraz regułach symlink/reparse (SEC-08). Ścieżka bezwzględna lub `..` poza rootem daje `permission_denied`. |
| MTL-21 | `file_write` przekazuje treść przez Artifacts hosta, gdy przekracza limit control. Limit jednego zapisu MCP to 8 MiB. |

### 7.9 Schematy szczegółowe

`chat_send`:

```json
{
  "name": "chat_send",
  "title": "Send a message to an agent chat",
  "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true},
  "inputSchema": {
    "type": "object",
    "additionalProperties": false,
    "required": ["chat", "text"],
    "properties": {
      "chat": {
        "type": "object", "additionalProperties": false, "required": ["chat_id"],
        "properties": {"host_id": {"type": "string", "format": "uuid"}, "chat_id": {"type": "string"}}
      },
      "text": {"type": "string", "minLength": 1, "maxLength": 65536},
      "mentions": {
        "type": "array", "maxItems": 32,
        "items": {"type": "object", "additionalProperties": false, "required": ["path"],
                  "properties": {"path": {"type": "string"}, "lines": {"type": "string", "pattern": "^[0-9]+(-[0-9]+)?$"}}}
      },
      "expected_history_epoch": {"type": "integer", "minimum": 0},
      "request_id": {"type": "string", "format": "uuid"}
    }
  },
  "outputSchema": {"$ref": "#/$defs/Receipt"}
}
```

`mentions` to pliki hosta względne do workspace rozmowy (CHAT-10). Obrazy i załączniki
są poza M1; wymagają gotowych obiektów Artifacts (CHAT-11). `expected_history_epoch`
chroni przed wysłaniem do innej gałęzi rozmowy niż oglądana przez agenta (OFF-03).

`decision_respond`:

```json
{
  "name": "decision_respond",
  "title": "Answer a pending agent decision",
  "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true},
  "inputSchema": {
    "type": "object",
    "additionalProperties": false,
    "required": ["chat", "run_id", "decision_id", "response"],
    "properties": {
      "chat": {"$ref": "#/$defs/ChatRef"},
      "run_id": {"type": "string"},
      "decision_id": {"type": "string"},
      "response": {
        "oneOf": [
          {"type": "object", "additionalProperties": false, "required": ["option_id"],
           "properties": {"option_id": {"type": "string"}}},
          {"type": "object", "additionalProperties": false, "required": ["action"],
           "properties": {
             "action": {"enum": ["allow_once", "allow_for_chat", "deny", "answer", "approve_plan", "keep_planning"]},
             "answers": {"type": "array", "items": {"type": "string"}, "maxItems": 16},
             "note": {"type": "string", "maxLength": 4096},
             "accept_edits": {"type": "boolean"}}}
        ]
      },
      "request_id": {"type": "string", "format": "uuid"}
    }
  },
  "outputSchema": {"$ref": "#/$defs/Receipt"}
}
```

`option_id` jest nieprzezroczystym ID opcji ACP (AGT-11). `action` odpowiada
`NativeDecisionAction` dla Claude i Codex. Gateway odrzuca akcję spoza opcji decyzji.

`Receipt` po wywołaniu wymagającym potwierdzenia:

```json
{
  "host_id": "5d0c…",
  "command_id": "0b9e…",
  "state": "awaiting_confirmation",
  "confirmation_id": "c-41",
  "resolved_by_user": false
}
```

`content` każdego wyniku zawiera też krótkie streszczenie tekstowe tego samego
`structuredContent` dla klientów bez obsługi `outputSchema`.

### 7.10 Preferencje dostępne przez MCP

Wszystkie metody `SettingsService` wymagają dziś `admin`, a principal MCP go nie dostaje.
Dlatego `settings_get` i `settings_update` korzystają z nowego, wąskiego handlera,
wspólnego z gRPC, z kontrolą zakresu innego niż `admin` (pytanie otwarte 10).
`agents_list` korzysta z `CatalogService` i nie wymaga `admin`.

`settings_update` (M3) obejmuje tylko preferencje hosta bez wykonywania kodu i bez sekretów,
np. domyślny agent i model nowych rozmów, zachowanie kolejki i powiadomień hosta.
Lista kluczy jest jawna w kodzie gatewaya. NIE obejmuje narzędzi, AgentDefinition,
profili, env, integracji, Remote, Automation ani preferencji klienta (motyw, zoom,
skróty, layout), które należą do urządzenia (APP-09).

## 8 Zasoby, subskrypcje i powiadomienia

Zasoby MCP są tylko do odczytu. Dają kontekst i wskaźniki zmian. Treść zasobu
ma te same limity i autoryzację co odpowiadające narzędzie R0.

| URI | Treść | Źródło zmian |
| --- | --- | --- |
| `canopy://hosts` | Wynik `hosts_list`. | ClientRegistry, WatchHost |
| `canopy://host/{host_id}/catalog` | Projekty, workspace, podsumowania czatów i terminali. | WatchSummaries |
| `canopy://host/{host_id}/decisions` | Oczekujące decyzje hosta. | WatchSummaries, SubscribeChat |
| `canopy://host/{host_id}/chat/{chat_id}` | `ChatSummary`, kolejka i ostatnia strona `chat_read`. | SubscribeChat |
| `canopy://host/{host_id}/terminal/{terminal_id}` | Wynik `terminal_read` bez scrollbacku. | TerminalSession (obserwator) |
| `canopy://host/{host_id}/operation/{command_id}` | `Receipt` operacji principala. | inbox, potwierdzenia |
| `canopy://host/{host_id}/workspace/{workspace_id}/git` | Wynik `git_status`. | WatchResource |

`resources/list` zwraca hosty i katalogi. Zasoby rozmów, terminali i operacji
są dostępne przez `resources/templates/list`, aby lista nie rosła z liczbą czatów.

| ID | Wymaganie |
| --- | --- |
| MRS-01 | `resources/subscribe` MUSI korzystać z istniejących strumieni hosta. Gateway ma jedną wewnętrzną subskrypcję na zasób i rozdziela zmiany do sesji MCP (SYNC-04). |
| MRS-02 | `notifications/resources/updated` MUSI być scalane do najwyżej 1 na sekundę na zasób i sesję. Powiadomienie nie zawiera treści; klient czyta zasób albo narzędzie z kursorem. |
| MRS-03 | Utrata powiadomień przez wolnego klienta NIE MOŻE zmienić poprawności odczytu. Po resync hosta gateway wysyła jedno powiadomienie aktualizacji zasobu. |
| MRS-04 | Zmiana zakresów, nowy host na liście principala i usunięcie zasobu MUSZĄ wysyłać `notifications/resources/list_changed` lub `notifications/tools/list_changed`. |
| MRS-05 | Gateway MOŻE wysyłać `notifications/message` (logging) tylko z kodem zdarzenia, zasobem i command_id. Bez promptów, treści plików i wyjścia terminala. |

## 9 Model błędów

Błędy protokołu i błędy domenowe są rozdzielone (RPC-02).

| Warstwa | Forma | Przykłady |
| --- | --- | --- |
| Transport HTTP | Status HTTP przed JSON-RPC | 401 brak tokenu, 403 obcy Origin, 404 nieznana sesja, 503 MCP wyłączone. |
| JSON-RPC | Obiekt `error` | -32700 parse, -32600 invalid request, -32601 nieznana metoda lub narzędzie, -32602 wejście niezgodne ze schematem, -32002 nieznany zasób. |
| Domena | Wynik narzędzia z `isError: true` i `structuredContent.error` | Wszystkie kody poniżej. |

| Kod | Znaczenie | `retryable` |
| --- | --- | --- |
| `permission_denied` | Brak zakresu, rootu, hosta na liście lub `automation_disabled`. | nie |
| `not_found` | Zasób nie istnieje lub nie jest widoczny dla principala. Te dwa przypadki są nierozróżnialne. | nie |
| `invalid_argument` | Wejście poprawne wg schematu, ale niezgodne z domeną. | nie |
| `precondition_failed` | Nieaktualny plan, hash, rewizja kolejki lub history epoch. | po odświeżeniu |
| `conflict` | Konflikt zapisu pliku, dirty presence, lease terminala innego klienta. | po decyzji |
| `idempotency_conflict` | Ten sam `request_id` z innymi argumentami. | nie |
| `already_resolved` | Decyzja rozstrzygnięta wcześniej; `details` zawiera wynik. | nie |
| `stale_run` | Odpowiedź lub Steer dla poprzedniego runu. | nie |
| `confirmation_denied` | Użytkownik hosta odmówił. | nie |
| `confirmation_expired` | Brak odpowiedzi w 5 min, restart, revoke lub koniec sesji. | tak, nowym wywołaniem |
| `unsupported` | Brak capability backendu, platformy lub hosta; `details.reason` (AGT-07). | nie |
| `needs_host_interaction` | Wymagany prompt OS na hoście (APP-11). | po działaniu na hoście |
| `host_unavailable` | Host offline lub trasa niedostępna; `details.last_seen`. | tak |
| `host_draining` | Host kończy pracę (LIFE-05). | nie |
| `rate_limited` | Limit gatewaya; `details.retry_after_ms`. | tak |
| `resource_exhausted` | Limit hosta: procesy, inbox, artefakty (PERF-01). | po zwolnieniu |
| `command_retired` | Komenda z zamkniętej epoki (CMD-10). | nie |
| `automation_paused` | Użytkownik włączył Pause automation. | po wznowieniu |
| `internal` | Błąd bez lepszej klasyfikacji; tylko bezpieczny opis. | zależnie |

| ID | Wymaganie |
| --- | --- |
| MER-01 | Gateway MUSI odwzorować `DomainError.code` i status gRPC na kody z tabeli. Nieznany kod domenowy daje `internal` z oryginalnym kodem w `details.code`. |
| MER-02 | Komunikat błędu NIE MOŻE zawierać ścieżek spoza zakresu principala, tokenów ani treści zasobów, do których principal nie ma dostępu. |
| MER-03 | `delivery_unknown` NIE jest błędem. Jest stanem `Receipt` z `isError: false` i wyjaśnieniem w `content`. |

## 10 Ryzyka bezpieczeństwa

| Ryzyko | Skutek | Kontrola |
| --- | --- | --- |
| Prompt injection przez treść zadania, pliku, PR lub transkryptu czytaną przez agenta MCP | Agent wykonuje polecenia napastnika: Push, odpowiedź na decyzję, zmiana zadania. | Minimalne zakresy, R2/R3 z potwierdzeniem na hoście (MCF-01), brak `approve` domyślnie, limity, audyt, Pause automation. Opisy narzędzi oznaczają treść zasobów jako dane niezaufane. |
| Eskalacja przez agenta rozmowy | Agent zatwierdza własne narzędzia lub tworzy rozmowy, które działają bez kontroli. | MCF-05; agent rozmowy bez `approve` domyślnie; rozmowy MCP mają pochodzenie `automation`; limit rozmów i Send per principal; zakaz zmian AgentDefinition i profili. |
| Pętla agentów | Rozmowy MCP tworzą kolejne rozmowy bez końca i zużywają limit 32 procesów. | Limit `chat_create`, głębokość pochodzenia 2 dla agentów rozmowy, `resource_exhausted` zamiast zabijania innych procesów. |
| Inny proces tego samego użytkownika | Proces czyta token z magazynu systemowego lub łączy się z gniazdem. | Granicą zaufania lokalnego jest konto OS. Token wymagany mimo ACL gniazda; nazwa klienta nie jest tożsamością; revoke per principal. Ryzyko pozostaje jawne. |
| DNS rebinding i CSRF na HTTP | Strona WWW wywołuje loopback MCP. | MTR-05..07: tylko loopback, Origin/Host, bearer token, brak cookies. |
| Wyciek sekretów | Token konta lub env w wyniku narzędzia lub logu. | SEC-06, MAU-01, MER-02, MAU-08; brak narzędzi sekretów i admin. |
| Wykonanie kodu przez konfigurację | Zmiana argv/env agenta lub narzędzia daje RCE. | Edycja AgentDefinition, narzędzi i profili jest zabroniona; `settings_update` ma listę dozwoloną. |
| Ścieżki poza rootem | Odczyt `~/.ssh` przez `file_read`. | MTL-20, SEC-08, rooty principala. |
| Zdalny host bez zgody właściciela | Automatyzacja na A steruje hostem B. | MHOST-01, MHOST-03, MHOST-04; zakresy B ograniczają A. |
| Powtórzenie skutku | Retry klienta tworzy drugi commit lub drugą wiadomość. | MTL-01, MTL-02, CMD-01..12. |
| Wyczerpanie zasobów | Flood wywołań, `chat_wait`, subskrypcji. | Sekcja 6.4, PERF-01, MTR-08. |
| Mylenie zgody | Użytkownik zatwierdza potwierdzenie bez zrozumienia skutku. | MCF-03: skutki z preflight, host docelowy i principal w treści; R3 bez opcji sesji. |

| ID | Wymaganie |
| --- | --- |
| MSEC-01 | Opisy narzędzi zwracających treść z zewnątrz MUSZĄ informować, że treść jest danymi, a nie poleceniami. Gateway NIE MOŻE wykonywać instrukcji z tej treści. |
| MSEC-02 | Agent rozmowy z Canopy control NIE MOŻE mieć `--allowedTools` ani automatycznej akceptacji dla narzędzi sterowania. Wywołanie narzędzia przechodzi przez zwykłą decyzję backendu w Canopy. Akceptacja Codexa dla serwera `canopy` (strony) NIE obejmuje serwera `canopy-control`. |
| MSEC-03 | Przegląd bezpieczeństwa M1 MUSI potwierdzić, że ścieżka sieciowa gRPC nie może wstawić principala MCP (MGW-01), a HTTP nie nasłuchuje poza loopback (MTR-05). |

## 11 Dane trwałe

| Dane | Magazyn |
| --- | --- |
| Principal automatyzacji | Istniejąca tabela zaufanych urządzeń: `kind`, MAC tokenu, lista hostów zdalnych, polityka potwierdzeń per narzędzie. |
| Inbox i dedupe | Istniejący inbox (CMD-01..12). Bez kopii. |
| Mapowanie command_id → principal dla hostów zdalnych | Istniejący outbox klienta urządzenia A z polem principala. |
| Dziennik audytu MCP | Nowa ograniczona tabela (MAU-08, MAU-09). Uzasadnienie SIMPLE-01: inbox nie zapisuje odczytów, odmów ani potwierdzeń. |
| Oczekujące potwierdzenia, sesje MCP, subskrypcje | Tylko pamięć gatewaya. |
| Token po stronie mostu | Magazyn systemowy bieżącego użytkownika, klucz nazwy klienta. |

Zmiany schematu podlegają DATA-01. Jeżeli MCP trafi do tego samego wydania co remote,
kolumny i tabela audytu dołączają do jedynej niewydanej migracji. Jeżeli remote zostanie
wydany wcześniej, MCP dostaje zwykłą nową migrację (DATA-02).

## 12 Etapy

| Etap | Wynik pracy | Bramka wyjścia |
| --- | --- | --- |
| M0 Kontrakt | Decyzja o kodeku MCP (rozszerzenie kodu `chat::mcp` albo biblioteka z uzasadnieniem SIMPLE-01/02). Prototyp MGW-01 i MGW-02. Most stdio z gniazdem. Generator `inputSchema`/`outputSchema` z typów Rust. | Wywołanie jednej usługi w procesie z principalem MCP przechodzi te same testy auth co gRPC; brak sieciowej drogi do principala. |
| M1 MVP lokalny | Parowanie stdio, principal, zakresy, potwierdzenia, audyt, Pause automation; narzędzia M1 z sekcji 7 na hoście lokalnym; zasoby `hosts`, `catalog`, `chat`, `decisions`, `operation`. | AC-36..AC-41 PASS; ręczny test z co najmniej dwoma klientami MCP. |
| M2 Wiele hostów i mutacje | Federacja przez ClientRegistry (MHOST-01..06), HTTP loopback, Git/worktree, pliki, terminal, Tasks odczyt, kolejka, subskrypcje i progress. | AC-42..AC-44 PASS; macierz L/RM/RW dla narzędzi M2. |
| M3 Pełny zakres | Agent rozmowy z Canopy control, `pr_create`, Tasks zapis, fork/goal, `settings_update`. | AC-45 PASS; pełny przegląd bezpieczeństwa i złożoności. |

M1 zależy od handlerów E1/E3 planu domenowego, które są już w kodzie. Publikacja
podlega zasadzie planu domenowego: nie publikujemy pośredniego protokołu remote.
Czy MCP M1 może trafić do wydania przed M2, jest pytaniem otwartym (sekcja 14).

### 12.1 Narzędzia MVP (M1)

`hosts_list`, `status_overview`, `operation_get`, `agents_list`, `projects_list`,
`chats_list`, `chat_create`, `chat_send`, `chat_read`, `chat_wait`, `chat_queue`,
`chat_interrupt`, `decisions_list`, `decision_respond`, `terminals_list`,
`terminal_read`, `git_status`, `git_diff`, `files_list`, `files_search`, `file_read`.

MVP pozwala zewnętrznemu agentowi uruchomić i prowadzić rozmowy, obserwować ich pracę,
odpowiadać na decyzje za potwierdzeniem człowieka i przeglądać wynik w Git i plikach.
Zmiany Git, plików i terminala pozostają w rękach agentów rozmów lub użytkownika do M2.

## 13 Odbiór

Reguły wyniku, poziomy dowodu (P, C, T, N, E, R) i stanowisko testowe są takie
jak w [macierzy odbioru](agent-runtime-remote-acceptance.md). Klient MCP w testach
to kontrolowany klient protokołu oraz co najmniej jeden rzeczywisty klient MCP
z wersją zapisaną w raporcie. Testy nie używają prawdziwych kont ani modeli
poza przypadkami E za zgodą.

| Test | Wymagania | Kroki i warunek PASS |
| --- | --- | --- |
| AC-36 P/T | MCP-01, MGW-01..05, MSEC-03 | Wywołaj tę samą operację czatu, Git i pliku przez UI, gRPC i MCP na tymczasowej bazie i repo. Wyślij przez gRPC żądanie z próbą podania principala MCP w metadanych. PASS: identyczny skutek i rekord inboxu; jedna implementacja handlera; gRPC bez certyfikatu odrzucone; MCP działa przy wyłączonym listenerze Remote. |
| AC-37 T | MTR-01..04, MAU-01..07 | Uruchom most bez aplikacji, przy wyłączonym MCP i w draining. Sparuj klienta, odmów drugiemu, przekrocz 5 min. Zamknij stdin podczas tury. Odczytaj dziennik i inbox. PASS: jawne błędy, brak danych przed zgodą, jedna zgoda na prośbę, token nie występuje w bazie, logach ani diagnostyce; zamknięcie stdin nie przerywa tury i nie rozstrzyga decyzji. |
| AC-38 T/E | MCF-01..06, MAU-08..10 | Wywołaj R2 i R3. Zatwierdź, odmów, zmień argumenty po potwierdzeniu, zrestartuj host, revoke w trakcie oczekiwania, Pause automation. Ustaw „bez potwierdzenia” dla jednego R2. PASS: brak skutku przed zgodą; zmiana argumentów wymaga nowej zgody; restart i revoke dają `confirmation_expired`; R3 zawsze pyta; dziennik zawiera decyzje bez treści argumentów. |
| AC-39 P/T/E | MTL-01..11, CHAT-01..05, F-03, F-05 | Utwórz czat przez MCP z jawnym kontem. Wyślij dwa razy z tym samym `request_id`, raz z inną treścią. Wyślij równocześnie z UI. Czekaj `chat_wait` na `needs_decision`, odpowiedz z MCP i z UI równocześnie. Przerwij połączenie w trakcie `chat_wait`. PASS: jedna wiadomość, `idempotency_conflict` dla zmiany, deterministyczna kolejka, jeden wynik decyzji i `already_resolved` dla drugiej strony, brak aktywacji taba hosta, pochodzenie `automation` widoczne w UI. |
| AC-40 T | MTL-15, MTL-19..21, F-11, F-13 (odczyt) | Czytaj plik z BOM/CRLF, plik binarny, symlink poza root, `..`, duży plik w stronach. Czytaj ekran TUI z alternate screen i szerokimi znakami. PASS: zachowane kodowanie i metadane, odmowa poza root, poprawne strony, tekst projekcji bez sekwencji ANSI. |
| AC-41 T | MER-01..03, sekcja 6.4 | Wywołaj każdy kod błędu z tabeli 9, przekrocz limity równoległości, mutacji i rozmiaru wyniku. Zasymuluj delivery_unknown. PASS: właściwa warstwa i kod, brak danych spoza zakresu w komunikatach, `rate_limited` z `retry_after_ms`, delivery_unknown jako stan. |
| AC-42 T/N | MHOST-01..06, OWN-04..05, MAU-04 | Klient MCP na A steruje A i B. Usuń B z listy principala, odłącz B, zmień trasę, wyłącz automatyzację dla A na B, zawęź zakresy A na B podczas subskrypcji. PASS: efektywne zakresy to część wspólna; host offline nie blokuje A; brak automatycznego ponowienia; B zapisuje pochodzenie `automation`; odcięcie w ≤1 s. |
| AC-43 T/E | MTL-12..14, MTL-16..18, APP-06..07, TERM-02..05 | Na tymczasowym repo wykonaj stage, commit z hookiem, Push do lokalnego remote i worktree create/remove przez MCP. Zmień HEAD po preflight. Pisz do terminala z lease innego klienta, utrać odpowiedź zapisu. PASS: nieaktualny plan odrzucony, jeden commit po retry, R3 wymaga zgody, lease nie jest przejmowany, bajty nie są powtarzane. |
| AC-44 T | MTR-05..08, MRS-01..05 | Uruchom HTTP z obcym Origin, bez tokenu, na adresie LAN. Subskrybuj 32 zasoby, nie odczytuj SSE, generuj zmiany. PASS: 403/401, brak listenera poza loopback, scalanie powiadomień, gateway i inne sesje działają, odczyt po resync jest poprawny. |
| AC-45 T/E/R | MCF-05, MSEC-01..02, MAU-02 | Uruchom agenta rozmowy z Canopy control. Spróbuj odpowiedzieć na decyzję własnej rozmowy, utworzyć rozmowy ponad limit i wywołać narzędzie bez decyzji backendu. Zakończ run. PASS: odmowa własnej decyzji, `rate_limited`/`resource_exhausted`, każde wywołanie przechodzi decyzję Canopy, token nieważny po końcu runu. Przegląd R potwierdza brak narzędzi admin i sekretów. |

## 14 Pytania otwarte

1. Czy MCP M1 może być wydany przed pełnym M2, jako funkcja wyłączona domyślnie,
   czy dopiero razem z wydaniem remote i macierzą platform?
2. Kodek MCP: rozszerzenie własnego kodu `chat::mcp` czy zewnętrzna biblioteka
   protokołu? Decyzja w M0 według SIMPLE-01/02 po ocenie obsługi Streamable HTTP.
3. Czy dodać OAuth 2.1 dla HTTP zgodnie z rozdziałem autoryzacji MCP, czy pozostać
   przy tokenie bearer na loopback? Obecny plan wybiera token, bo endpoint jest lokalny.
4. Czy host B powinien mieć osobny poziom zaufania dla automatyzacji (MHOST-04) jako
   domyślnie wyłączony, czy domyślnie włączony dla urządzeń z pełną zgodą `*`?
5. Czy `chat_send` w M2 ma obsługiwać obrazy przez Artifacts, czy pozostawić je
   klientom gRPC i UI?
6. Czy nowsze mechanizmy MCP dla długich operacji zastąpią `operation_get`
   i `chat_wait`, gdy będą stabilne i obsługiwane przez docelowych klientów?
7. Czy zatwierdzenie R2 „dla tej sesji” jest potrzebne, czy wystarczy polityka
   per narzędzie ustawiana w Preferences?
8. Jaki jest domyślny zestaw zakresów agenta rozmowy z Canopy control:
   tylko `read`, czy `read` i `chat` w obrębie projektu rozmowy?
9. Czy klient mobile ma mieć widok potwierdzeń MCP hosta, czy potwierdzenia
   pozostają wyłącznie na desktopie, który wykonuje gateway?
10. Jaki zakres ma chronić wąski odczyt i zapis preferencji hosta przez MCP:
    `read`/`chat`, nowy zakres `preferences`, czy ustawienia pozostają tylko w UI?
11. Czy rozmowy utworzone przez MCP mają domyślną głębokość pochodzenia 2
    i limit 8 równoczesnych rozmów na principal, czy wystarczy limit `chat_create`?

## 15 Mapa kodu i źródła

Punkty wejścia zweryfikowane na `rust-rewrite`, baza `7547b1a`:

- [`chat::mcp`](../src/chat/mcp.rs), [`chat::pages`](../src/chat/pages.rs) i
  [`chat::launch`](../src/chat/launch.rs): obecny serwer MCP stron, token per proces
  czatu i wpinanie serwera do Claude/Codex.
- [`remote::server`](../src/remote/server.rs) i [`remote::auth`](../src/remote/auth.rs):
  `ServerServices`, `RpcAuth::authenticate`, `scope`, `resource`, `input`.
- [`host`](../src/host.rs) i [`host::authorization`](../src/host/authorization.rs):
  zakresy i `DeviceGrant`.
- Usługi do ponownego użycia: [czat](../src/app_state/remote/chat.rs),
  [katalog](../src/app_state/remote/catalog.rs), [ustawienia](../src/remote/settings_rpc.rs),
  [Tasks](../src/remote/tasks.rs), [terminal](../src/remote/terminal_rpc.rs),
  [Git](../src/remote/resources/git.rs), [pliki](../src/remote/resources/files.rs),
  [artefakty](../src/remote/artifact_rpc.rs), [host](../src/remote/host.rs).
- [`remote::client`](../src/remote/client/mod.rs): `ClientRegistry` i `HostClient`
  dla wielu hostów.
- [PR w UI](../src/app_state/pull_requests.rs) i
  [GitHub pulls](../src/integrations/github/pulls.rs): handler do wydzielenia dla `pr_create`.
- [Kontrakt Protobuf](../proto/canopy/remote/v1/canopy.proto).

Źródła protokołu MCP:

- [Transporty stdio i Streamable HTTP](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports)
- [Narzędzia](https://modelcontextprotocol.io/specification/2025-06-18/server/tools) i
  [zasoby](https://modelcontextprotocol.io/specification/2025-06-18/server/resources)
- [Progress](https://modelcontextprotocol.io/specification/2025-06-18/basic/utilities/progress) i
  [anulowanie](https://modelcontextprotocol.io/specification/2025-06-18/basic/utilities/cancellation)
- [Autoryzacja](https://modelcontextprotocol.io/specification/2025-06-18/basic/authorization) i
  [dobre praktyki bezpieczeństwa](https://modelcontextprotocol.io/specification/2025-06-18/basic/security_best_practices)
