# Abstrakcja agentów i zdalne sterowanie Canopy

Status: plan docelowy do implementacji i odbioru. Data: 2026-10-25.
Zmieniony 2026-10-31 zgodnie z [ADR 0001](adr/0001-thread-architecture.md).
Plan uwzględnia wspólne opracowanie i recenzje Opus 5.5 oraz Astra 6.

Canopy ma udostępniać te same operacje lokalnemu desktopowi, zdalnemu desktopowi,
aplikacji mobilnej i MCP. Jeden runtime hosta wykonuje operacje i rozstrzyga konflikty.
Model zna tylko wątek. Lokalny lub zdalny jest wyłącznie transport klienta.
Adaptery agentów oddzielają protokoły Claude, Codex i ACP od modelu rozmowy.
Transport między urządzeniami to gRPC z Protobuf.

To jedno przedsięwzięcie i jedno kompletne wydanie. Etapy określają kolejność pracy.
Zaliczenie etapu nie oznacza ukończenia funkcjonalności ani zgody na jej publikację.
[Macierz odbioru](agent-runtime-remote-acceptance.md) jest częścią tego kontraktu.

## 1 Zakres i sposób czytania wymagań

`MUSI` oznacza warunek odbioru. `NIE WOLNO` oznacza zakaz. `MOŻE` pozostawia wybór
implementacji, pod warunkiem zachowania opisanych wyników. Każde wymaganie ma stały
identyfikator. Test może sprawdzać kilka wymagań. Liczba testów nie jest celem.

Tekst stosuje krótkie zdania, stałe nazwy i jawnych wykonawców czynności.
Jest inspirowany ASD-STE100. Nie deklaruje formalnej zgodności polskiego tekstu
z kontrolowanym językiem angielskim ASD-STE100.

| ID | Wymaganie |
| --- | --- |
| SCOPE-01 | Wydanie MUSI objąć wszystkie wiersze zakresu w sekcji 3, wraz z błędami, odtwarzaniem i cleanupem. |
| SCOPE-02 | Host MUSI działać tylko podczas działania aplikacji Canopy. Utrata klienta nie kończy pracy hosta. Quit hosta zachowuje dotychczasowy kontrakt zakończenia aplikacji. |
| SCOPE-03 | Remote MUSI działać przez LAN, tunel SSH i Tailscale lub inny VPN. Wydanie nie wymaga publicznego serwera, konta w chmurze ani przekaźnika. |
| SCOPE-04 | Klientami są natywne aplikacje desktop i mobile. Klient web, daemon oraz uruchamianie headless są poza tym wydaniem. Model domeny i backendy nie mogą wymagać okna. |
| SCOPE-05 | Natywne integracje Claude i Codex MUSZĄ zachować obecne funkcje. ACP MUSI być trzecią obsługiwaną rodziną protokołu. |
| SCOPE-06 | Niewspierana funkcja konkretnego agenta lub platformy MUSI mieć dokładny powód. Flaga capability nie może zastąpić implementacji funkcji wymaganej dla wspieranej kombinacji. |

Podstawą regresji są aktualny kod i [kontrakty projektu](project-contracts.md).
Ten plan zmienia własność runtime'u i dodaje zdalny dostęp. Nie zmienia samodzielnie
reguł usuwania rozmów, bezpieczeństwa Git, uprawnień agentów ani wyglądu Canopy.
Zmiana istniejącej reguły wymaga jawnej poprawki tego planu i odpowiedniego kontraktu.

## 2 Wymóg ograniczenia złożoności

| ID | Wymaganie |
| --- | --- |
| SIMPLE-01 | Autor nowej warstwy, traita, crate'a, tabeli lub zależności MUSI wskazać wymaganie tego wydania, któremu ta konstrukcja służy. |
| SIMPLE-02 | Autor MUSI porównać nową konstrukcję z najprostszym rozwiązaniem opartym na istniejącym kodzie. Uzasadnienie MUSI wskazać usuwany duplikat, wspólny lifecycle, kontrolę konkretnej awarii albo obsługę wymaganych implementacji. |
| SIMPLE-03 | Samo powołanie się na czysty kod, SOLID, przyszłą elastyczność lub dobre praktyki NIE spełnia SIMPLE-02. |
| SIMPLE-04 | Implementacja NIE MOŻE dodawać event sourcingu całej aplikacji, CRDT całego stanu, systemu pluginów binarnych, automatycznego failoveru hostów ani własnej kryptografii. |
| SIMPLE-05 | Lokalny desktop MUSI wywoływać tę samą szynę komend co gRPC i MCP, przez transport `Local`. Nie musi kodować Protobuf ani łączyć się z własnym serwerem. |
| SIMPLE-06 | Refactor MUSI korzystać z istniejących workerów SQLite, libgit2, plików i lifecycle procesów. Nowy worker jest dopuszczalny, jeśli istniejący nie zapewnia wymaganego cyklu życia lub izolacji opóźnień. |
| SIMPLE-07 | Podział na crate'y i wybór enum albo trait pozostają decyzją implementacji. Nie są samodzielnymi kryteriami jakości. |

Minimalne potrzebne granice to: model wątku, backend protokołu, właściciel runtime'u,
szyna komend, handlery istniejących funkcji, `ThreadTransport` oraz adaptery gRPC i MCP.
Jedna szyna komend jest wymagana. Jest wspólna dla UI, gRPC i MCP (BUS-01..05,
[ADR 0001](adr/0001-thread-architecture.md) D3). Nie jest wymagany repository pattern
nad SQLite ani uniwersalny framework synchronizacji.

| ID | Wymaganie |
| --- | --- |
| BUS-01 | UI, adapter gRPC i adapter MCP MUSZĄ wykonywać mutacje przez jedną szynę komend. Adapter tylko dekoduje wejście i woła szynę. |
| BUS-02 | Szyna MUSI wykonywać autoryzację, dedupe, fingerprint i klasyfikację ryzyka. Adapter NIE MOŻE powtarzać ani pomijać tych kontroli. |
| BUS-03 | Rejestr komend w Rust MUSI być źródłem prawdy dla nazwy, klasy ryzyka i zakresu uprawnień każdej komendy. |
| BUS-04 | Zmiana ustawień MUSI być komendą szyny. Wynik zawiera zatwierdzoną wartość i nową rewizję. |
| BUS-05 | Każda komenda dostępna w UI MUSI mieć RPC w `canopy.api.v1` i narzędzie MCP. Brak jednego z nich jest błędem kontraktu. |

## 3 Pełny zakres funkcjonalny

Każdy poniższy wiersz wymaga odbioru lokalnego, remote desktop i remote mobile.
Warianty platformowe określa sekcja 14. Ograniczenia muszą wynikać z platformy lub
protokołu, a nie z niedokończenia jednego klienta.

| Funkcja | Docelowe zachowanie |
| --- | --- |
| F-01 Połączenia | Dodanie hosta, parowanie, zaufanie, reconnect, zmiana trasy, odwołanie urządzenia, usunięcie połączenia. |
| F-02 Środowiska | Kilka hostów jednocześnie; projekty, checkouty i prywatne rozmowy; zbiorcza lista pracy oraz decyzji. |
| F-03 Rozmowy | Utworzenie, nazwa, Send, kolejka, edycja kolejki, kolejność, Steer, Stop, Resume kolejki, Retry, usunięcie. |
| F-04 Backend | Wybór konta przed pierwszą wiadomością, modele, reasoning, tryby, konfiguracja i błędy autoryzacji. |
| F-05 Zgody | Narzędzia, pytania i plan; zakres pamiętania zgody; równoczesne odpowiedzi z różnych klientów. |
| F-06 Ciągłość rozmowy | Historia, restart, konkretne resume, fork i edit; rewind plików tylko tam, gdzie zapewnia go backend. |
| F-07 Aktywność | Thinking, narzędzia, diffy, plan pracy, subagenci, ich podwątki, praca w tle i goal. |
| F-08 Composer | Obrazy, ścieżki hosta, cytaty, fragmenty kodu, komendy i skille, wzmianki plików i PR, lokalne szkice. |
| F-09 Narzędzia Canopy | Shell `!`, strony HTML, podgląd strony, otwieranie plików i diffu z rozmowy. |
| F-10 Czytanie | Paginacja historii, wyszukiwanie całej zapisanej rozmowy, nawigator, kopiowanie, statusy i powiadomienia. |
| F-11 Files i editor | Drzewo, wyszukiwanie plików, tworzenie, odczyt, edycja, zapis, konflikt, reload, ochrona dirty buffer. |
| F-12 Git | Changes, stage/unstage/discard, diff, commit z hookami i podpisem, historia, refs, Pull/Push i worktree. |
| F-13 Terminal | Uruchomienie, wejście, paste, resize, zaznaczenie, kopiowanie, scrollback, restart, Stop i cleanup. |
| F-14 Tasks | Istniejące operacje GitHub, Jira i YouTrack, filtry, załączniki, edycja zadań, powiązanie i task do worktree. |
| F-15 Ustawienia | Preferencje hosta, katalog agentów i profili, integracje, zapis sekretu bez odczytu zwrotnego; preferencje klienta osobno. |
| F-16 Widoki | Niezależna nawigacja, wiele okien, obrazy/fonty/wideo/diffy/notes i strony na obsługiwanych platformach. |
| F-17 Zakończenie | Stop zasobu/worktree, usunięcie, retry cleanupu, Quit hosta, utrata klienta i restart hosta. |

Zakres oznacza udostępnienie istniejących operacji Canopy. Nie dodaje nowych funkcji
biznesowych, np. merge edytorów, LSP, administracji Jira lub synchronizacji plików
między komputerami. Mobile korzysta z układu dostosowanego do dotyku. Nie odtwarza
obrazu okna desktopu i nie wymaga używania zdalnej myszy.

## 4 Pojęcia i właściciele stanu

| Pojęcie | Znaczenie i tożsamość |
| --- | --- |
| Host | Instancja danych Canopy na komputerze. Ma trwały `HostId`. Adres sieciowy nie jest tożsamością. |
| Połączenie | Konfiguracja klienta: oczekiwany HostId, zaufana tożsamość TLS, trasy i odwołanie do własnego credentialu. |
| Środowisko | Host wykonujący wątki. Tożsamością środowiska jest tylko `HostId`. |
| Projekt i checkout | Istniejący projekt/workspace na konkretnym hoście. Dla wątku jest metadaną, a nie częścią tożsamości. Ten sam origin na dwóch hostach nie scala zasobów. |
| AgentDefinition | Stabilne ID, nazwa, profil i rodzina protokołu. Wiele definicji może używać tego samego backendu ACP. |
| Backend | Implementacja protokołu: Claude stream-json, Codex app-server albo ACP. Nie jest dostawcą modelu. |
| Wątek (Thread) | Pełna jednostka pracy: środowisko, agent, gałąź lub worktree, rozmowa i metadane. Ma globalny `ThreadId` (UUIDv7), `revision` i `executor_epoch`. Istnieje niezależnie od liczby oglądających klientów. Stare `chat.id` wskazuje `thread_alias`. |
| ThreadTransport | Sposób dostępu klienta do wątku: `Local` w procesie albo `Grpc`. Nie jest częścią modelu wątku. |
| executor_epoch | Epoka wykonawcy wątku. Służy do fencingu komend. Jest odrębna od command_epoch urządzenia, runtime_epoch strumienia i runu. |
| Run | Jedno uruchomienie procesu rozmowy. Ma nowy, niepowtarzalny identyfikator generacji. |
| Provider session | Nieprzezroczysty identyfikator Claude/Codex/ACP. Backend interpretuje go przy resume. |
| History epoch | Istniejąca część historii oddzielona np. restartem lub edycją. Nie jest generacją procesu ani kursorem sieciowym. |
| ResourceRef | Para `HostId` i typowany identyfikator zasobu. Wątek używa `ThreadRef{host_id, thread_id}`. Pozostałe zasoby zachowują dotychczasowe ID. |
| View | Prezentacja zasobu na danym urządzeniu. Okno renderuje stan globalny aplikacji. Fokus, tab, scroll i lokalne PaneId nie sterują procesem. |

| ID | Wymaganie |
| --- | --- |
| OWN-01 | Host MUSI mieć jednego właściciela każdego runtime'u czatu i terminala. Liczba klientów nie zwiększa liczby procesów. |
| OWN-02 | Runtime MUSI posiadać proces, pompę zdarzeń, kolejkę, decyzje, generację, zapis historii oraz operacje cleanupu. |
| OWN-03 | Widok MUSI posiadać fokus, kontrolki, ruch, scroll, zaznaczenie i szkic. Nie może być warunkiem przetwarzania wiadomości lub zgody. |
| OWN-04 | Celem każdej komendy i odpowiedzi MUSI być `(ThreadId, executor_epoch)` ustalony w chwili utworzenia komendy. Zmiana aktualnej nawigacji nie przekierowuje operacji. |
| OWN-05 | Klient MUSI móc obserwować zasoby kilku hostów. Niedostępny host nie blokuje odczytów i mutacji pozostałych hostów. |
| OWN-06 | Runtime MUSI odrzucić spóźniony wynik poprzedniego runu. Restart i powtórne podłączenie klienta nie mogą go zastosować do nowego runu. |
| OWN-07 | Utworzenie czatu zdalnie MUSI dodać go do katalogu hosta bez aktywowania jego taba lub okna. Użytkownik hosta widzi go w odpowiednim projekcie lub Private chats. |
| OWN-08 | HostId i identyfikatory zasobów MUSZĄ pozostać stabilne po restarcie i wdrożeniu. Identyfikatora usuniętego zasobu nie wolno użyć ponownie na tym hoście. |
| OWN-09 | Agent wątku MUSI działać w środowisku ustalonym przy utworzeniu wątku. Przeniesienie wykonania na inny host NIE jest częścią tego wydania; jawny handoff z fencingiem `executor_epoch` to późniejszy etap. |
| OWN-10 | Aplikacja MUSI mieć jeden stan globalny: projekty, wątki i układ. Okna tylko go renderują. Akcja widoku zmienia stan globalny, a nie prywatny stan okna. |
| OWN-11 | Użytkownik MUSI mieć jedną instancję aplikacji. Druga instancja NIE MOŻE pokazywać innego stanu; przekazuje aktywację działającej instancji albo kończy się z jawnym komunikatem. |

Runtime MOŻE być encją GPUI należącą do aplikacji. Czyste modele i backendy pozostają
bez GPUI. Sieciowy executor przekazuje typowane polecenia do właściciela stanu.
Nie otrzymuje dostępu do `Window` ani nie mutuje encji z obcego wątku.

## 5 Kontrakt backendu agentów

### 5.1 Granica protokołu

Backend przyjmuje intencje: start, send, steer, interrupt, decyzję, zmianę opcji,
resume, fork i operacje dodatkowe. Zwraca zdarzenia modelu i efekty do wykonania.
Własny stan handshake, request ID, wire JSON i interpretacja odpowiedzi należą do backendu.
Rodzina protokołu może być zamkniętym enumem wewnętrznym. Lista AgentDefinition
i kont dostępnych w UI musi być danymi, a nie enumem nazw agentów.

| ID | Wymaganie |
| --- | --- |
| AGT-01 | Wspólny model i UI NIE MOGĄ importować typów wire Claude/Codex/ACP ani rozpoznawać nazwy dostawcy w celu wyboru zachowania. Branding pochodzi z opisu agenta. |
| AGT-02 | Backend MUSI tłumaczyć narzędzia na rodzaj semantyczny, stan, tytuł, lokalizacje i treść. Oryginalna nazwa oraz surowe dane mogą pozostać materiałem diagnostycznym. |
| AGT-03 | Nieznane narzędzie MUSI mieć ogólną kartę. Nieznane zdarzenie protokołu nie może przerwać sesji; backend raportuje ograniczoną diagnostykę. Błędny handshake kończy start jawnym błędem. |
| AGT-04 | Backend MUSI zwracać capabilities oraz opisy opcji z ID, typem, zakresem, wartościami i bieżącym wyborem. UI nie może narzucać wszystkim agentom enumów Mode lub Effort. |
| AGT-05 | Zmiana opcji MUSI określać moment działania: od razu, od następnej tury albo po restarcie. Odmowa backendu zachowuje poprzedni zatwierdzony wybór i szkic. |
| AGT-06 | Katalog modeli i komend MUSI być scoped do hosta, definicji agenta oraz konta. Jego odświeżanie nie może blokować Send dla gotowej sesji. |
| AGT-07 | Brak capability MUSI wyłączyć daną akcję z konkretnym powodem. Nie wolno emulować fork, rewind, Steer ani goal przez ukryty prompt o innym znaczeniu. |
| AGT-08 | Generic ACP MUSI negocjować obsługiwaną wersję i capabilities. Dodanie kolejnego zgodnego agenta przez command, argv, env i profil nie może wymagać zmian w modelu ani UI. |
| AGT-09 | Integracja ACP MUSI obsługiwać tekst, narzędzia, plan, decyzje, cancel, konfigurację, obrazy i resume wtedy, gdy agent je ogłasza. Nie wolno traktować nieogłoszonej funkcji jako dostępnej. |
| AGT-10 | Klient ACP MUSI implementować i ogłaszać fs/read_text_file, fs/write_text_file oraz terminal. Metody MUSZĄ korzystać z istniejących usług hosta, zakresu sesji i reguł poniżej. Nie wolno ogłaszać capability przed zaliczeniem jej kontraktu. |
| AGT-11 | ID opcji zgody i konfiguracji ACP MUSZĄ pozostać nieprzezroczyste. Backend nie może odgadywać ich znaczenia wyłącznie po etykiecie. |
| AGT-12 | Uruchomienie MUSI zachować środowisko profilu, redakcję sekretów, prywatne pliki, walidację argv i bounded cleanup. Render nie wykonuje I/O. |

ACP v1 jest bazową wersją tego planu. Wersja CLI i schemat implementowanego protokołu
muszą być zapisane w raporcie odbioru. Obsługa innej wersji wymaga adaptera tej wersji
i tych samych testów kontraktowych. Nie jest wymagana zgodność z każdą wersją roboczą.

ACP odczytuje plik z bufora edytora otwartego lokalnie na hoście, jeżeli taki bufor
istnieje, a w przeciwnym razie z dysku hosta. Nie pobiera buforów innych urządzeń.
Zakres linii jest stosowany do tego samego źródła. Zapis ACP używa handlera SaveFile:
ustala wersję bazową, sprawdza cel i dirty presence, następnie stosuje reguły APP-02.
Dirty buffer na dowolnym urządzeniu daje agentowi jawny błąd, bez nadpisania bufora.
Nie jest wymagane przekazywanie przez agenta hasha, którego metoda ACP nie zawiera.

Terminal ACP jest zasobem runu agenta. Agent zarządza create, output, wait_for_exit,
kill i release. Użytkownicy mogą obserwować wynik, bez input i resize. Koniec tury
nie kończy terminala, który zgodnie z protokołem pracuje w tle. Release albo koniec
runu uruchamia wspólny cleanup. Zachowany wynik narzędzia pozostaje w historii.

Przy session/load backend oddziela replay od nowych zdarzeń do zakończenia odpowiedzi
load. Gdy Canopy ma checkpoint tej sesji, replay nie dopisuje historii i nie tworzy
nowego history_epoch. Replay może odtworzyć historię tylko przy jej braku, atomowo
po udanym load. Niezgodność lub przerwany load daje błąd, bez częściowego importu.
Aktualne opcje sesji można odświeżyć niezależnie od replay transkryptu.
Obsługiwane session/resume bez replay ma pierwszeństwo przy istniejącej historii.
AuthMethods i authenticate należą do backendu: profile/env używają konta hosta,
a wymagane logowanie interaktywne zwraca needs_host_interaction. Sam wybór metody
autoryzacji nie może zmienić konta istniejącej rozmowy.

### 5.2 Wiadomości, kolejka i decyzje

| ID | Wymaganie |
| --- | --- |
| CHAT-01 | Host MUSI ustalić kolejność przyjętych wiadomości. Jedna wiadomość ma jedno ID klienta i jeden element użytkownika niezależnie od liczby prób RPC. |
| CHAT-02 | Kolejka MUSI być trwałym wspólnym stanem hosta. Send zapisuje pozycję razem z wynikiem komendy. Edycja, usunięcie i zmiana kolejności wymagają aktualnej rewizji kolejki; konflikt odświeża widok bez utraty lokalnej edycji. |
| CHAT-03 | Steer MUSI zachować znaczenie backendu i potwierdzenie wczytania. Bez wsparcia wiadomość może trafić do zwykłej kolejki tylko po jawnym wyborze użytkownika. |
| CHAT-04 | Stop MUSI odrzucić oczekujące decyzje, przerwać bieżącą pracę zgodnie z backendem i wstrzymać kolejkę. Resume kolejki zachowuje dotychczasowe RESUME_PROMPT i reguły celu. |
| CHAT-05 | Decyzja MUSI mieć ID, run i stan pending. Pierwsza ważna odpowiedź wygrywa; kolejne otrzymują already_resolved z aktualnym wynikiem. Odpowiedź starego runu jest odrzucona. |
| CHAT-06 | Zapamiętana zgoda MUSI zachować istniejący zakres chat i jego czas życia w Canopy. Nie wolno dodać nowego zakresu run ani zapisać zgody globalnie lub do projektu jako skutku tej funkcji. |
| CHAT-07 | Resume MUSI wskazywać konkretną sesję tego samego backendu i konta. Błąd nie może uruchomić nowej sesji po cichu. Nie wolno wymagać UUID od zewnętrznego ID poza adapterem, który tego wymaga. |
| CHAT-08 | Edit i fork MUSZĄ zachować źródłową historię, punkt rozgałęzienia i załączniki. Przeniesienie wykonania na inny host nie jest operacją fork w tym wydaniu. |
| CHAT-09 | Goal, subagenci i praca w tle MUSZĄ zachować obecne reguły zakończenia tur i statusu idle. Zakończenie jednego narzędzia nie może skasować innej oczekującej decyzji. |
| CHAT-10 | Komendy, skille, wzmianki plików i PR oraz wynik shella MUSZĄ być składane dla agenta na hoście. Cytaty i kod zachowują treść wskazaną przez użytkownika. |
| CHAT-11 | Załącznik MUSI być gotowym obiektem hosta przed dispatch wiadomości. Ścieżka pliku z telefonu nie może być interpretowana jako ścieżka hosta. |
| CHAT-12 | Wyszukiwanie MUSI obejmować całą zachowaną historię, także poza załadowaną stroną. Wynik zawiera stabilny punkt do doładowania. Skrócony zapis musi ujawniać ograniczenie wyszukiwania. |

Zakres zgody musi być potwierdzony przez backend. Opcji ACP typu allow_always nie wolno
oferować jako „dla tej rozmowy”, jeżeli backend nie potwierdza takiego ograniczenia.
Wtedy dostępna pozostaje decyzja jednorazowa, a ograniczenie jest widoczne.

Pozycja kolejki ma trwałe ID, treść lub referencje do artefaktów, kolejność i stan
queued, attempting, delivered, failed, dismissed albo delivery_unknown. Dispatcher zapisuje
attempting przed przekazaniem do stdin. Delivered wymaga dowodu przyjęcia określonego
przez backend; samo Process.send nie wystarcza. Po awarii attempting bez dowodu daje
delivery_unknown i nie jest ponawiane automatycznie, także gdy sam Send zakończył
się wcześniej succeeded. Gdy backend nie ma jawnego ACK, pierwsze jednoznacznie
skorelowane zdarzenie tury może stanowić dowód. Brak takiego dowodu pozostaje niepewnością.

Po restarcie queued wracają jako wstrzymane. Samo otwarcie taba ani wznowienie procesu
nie wysyła ich. Jawny Resume rewaliduje konto i kontekst. Edycja lub usunięcie dotyczy
tylko niewysłanych pozycji; niepewna pozycja wymaga świadomego rozstrzygnięcia.
Quit zachowuje kolejkę. Dotychczasowy prompt o jej odrzuceniu zostaje usunięty.
Jest to jawna zmiana obecnego kontraktu kolejki, wymagana przez trwały zdalny Send.

Delivery_unknown blokuje dispatch następnych pozycji, także po Resume. Użytkownik
z zakresem chat i aktualną rewizją kolejki wybiera dokładnie jedną z trzech akcji:
uznaj za dostarczoną, wyślij ponownie albo odrzuć. Pierwsza zapisuje delivered
z oznaczeniem decyzji użytkownika, bez udawania dowodu backendu. Druga archiwizuje
starą pozycję i atomowo tworzy nową komendę oraz nowe ID pozycji w tym samym miejscu
kolejki. Ostrzeżenie wyjaśnia możliwość powtórnego skutku. Trzecia zapisuje dismissed.
W każdym wariancie zachowujemy treść i powód rozstrzygnięcia. Dopiero potem kolejka
może przejść dalej; wstrzymana nadal wymaga Resume.

## 6 Cykl życia i zdalne widoki

| Operacja | Skutek |
| --- | --- |
| Attach lub Detach remote | Zmienia subskrypcję klienta. Nie uruchamia automatycznie uśpionego agenta i nie kończy już uruchomionego. |
| Pokazanie lokalnego taba | Zachowuje obecny lazy start procesu; nie uruchamia wstrzymanej kolejki po restarcie. |
| Start lub Send | Jawnie uruchamia potrzebny runtime. Może działać bez lokalnego ChatView. |
| Interrupt | Zatrzymuje turę według CHAT-04. Nie usuwa rozmowy. |
| Restart | Zamyka poprzedni proces i tworzy nowy run. Zachowuje rozmowę i konkretne resume zgodnie z backendem. |
| Close remote view | Usuwa tylko odwołanie w layoucie klienta. Nie jest Delete zasobu. |
| Close host tab lub Delete chat | Obejmuje rozmowy wskazanych pane'ów albo wskazany ThreadId. Zachowuje guard usuwania; inni klienci dostają resource_deleted. |
| Close host project | Obejmuje wszystkie rozmowy projektu z katalogu, także bez pane'a na hoście. Nie usuwa rozmów innego projektu. |
| Stop worktree | Łączy zasoby danego hosta i worktree; czeka na wspólny wynik cleanupu. |
| Quit host | Zamyka runtime'y i zapis. Nie usuwa rozmów ani danych projektów. |

| ID | Wymaganie |
| --- | --- |
| LIFE-01 | Klient MUSI odróżnić zamknięcie własnego widoku od usunięcia zasobu hosta. Etykieta i potwierdzenie muszą ujawniać zakres. |
| LIFE-02 | Utrata sieci, deadline RPC i zamknięcie klienta NIE MOGĄ wywołać Interrupt, Delete ani odrzucenia oczekującej zgody. |
| LIFE-03 | Usunięcie MUSI przejść przez istniejącą ochronę historii, kolejki, dirty buffer i procesów. Potwierdzenie wymaga opisu skutków i aktualnych warunków. |
| LIFE-04 | Cleanup MUSI mieć jednego właściciela oraz wspólny wynik dla wszystkich żądań. Błąd zachowuje zasoby i możliwość Retry; nie wolno raportować sukcesu przed potwierdzeniem zakończenia. |
| LIFE-05 | Quit MUSI przełączyć host w draining, odrzucić nowe mutacje i poczekać na istniejący zapis oraz cleanup. Błąd zachowuje aplikację otwartą z możliwością ponowienia. |
| LIFE-06 | Zdalny klient NIE MOŻE zakończyć aplikacji hosta. Może zamknąć zasób lub zatrzymać worktree w swoim zakresie uprawnień. |
| LIFE-07 | Restart hosta MUSI odtworzyć metadane i historię bez automatycznego ponowienia niepewnych efektów. Obowiązuje dotychczasowy lazy start oraz jawny start zdalny. |
| LIFE-08 | Host offline MUSI mieć last_seen i oznaczenie nieaktualnego cache. Nie wolno zmienić ostatniego statusu agenta na failed wyłącznie z powodu braku hosta. |
| LIFE-09 | Proces bez widoków MUSI przejść w dormant po 60 s bez tury, decyzji, trwającego dispatch, aktywnej kolejki i pracy w tle, jeżeli backend umożliwia konkretne resume. Trwała wstrzymana kolejka oraz delivery_unknown nie blokują uśpienia. Kolejny Start/Send wznawia tę samą sesję. Limit 32 procesów czatu daje resource_exhausted zamiast tworzenia procesu ponad limit. |
| LIFE-10 | Guard projektu MUSI odczytać katalog i runtime, a nie listę ChatView. Preflight MUSI wymienić objęte rozmowy, ich pracę, kolejki i liczbę obserwujących klientów, także dla rozmów utworzonych wyłącznie zdalnie. |

Preflight usunięcia jest operacją domenową. Nie zawiera callbacku do `Window`.
Odpowiedź wskazuje zasoby, skutki i warunki ważności. Klient prezentuje istniejące
potwierdzenia. Host ponownie sprawdza warunki bezpośrednio przed skutkiem.
Backend bez konkretnego resume nie jest automatycznie usypiany. Nadal podlega
limitowi procesów i jawnemu Stop/Restart/Delete. Samo odłączenie obserwatora nie
zmienia zgody ani nie przerywa aktywnej pracy.
Ukryty lokalny tab nadal jest widokiem. Samo przełączenie taba nie usypia procesu;
to zachowuje obecne zachowanie lokalne. Limit 32 procesów jest nowym, jawnym
ograniczeniem tego wydania, wspólnym dla lokalnego i zdalnego startu. Trzydziesty
trzeci tab nadal pokazuje historię i szkic, ale auto-start i Send zwracają limit
z możliwością zatrzymania wybranej sesji i ponowienia. Nie kończymy innego procesu
bez decyzji użytkownika. Wstrzymana kolejka nie znika przy dormant.

## 7 Dane trwałe i jedna migracja wydaniowa

Wymóg jednej migracji dotyczy historii wydania. Nie oznacza zamrożenia projektu
schematu przed implementacją. Niewydaną migrację można poprawiać wraz z projektem.
Nie wolno publikować kolejnych migracji wyłącznie dlatego, że praca została podzielona
między etapy lub agentów.

| ID | Wymaganie |
| --- | --- |
| DATA-01 | Całe przedsięwzięcie MUSI mieć jedną migrację wydaniową. Etapy deweloperskie MUSZĄ poprawiać tę samą niewydaną migrację zamiast tworzyć ciąg schematów przejściowych. |
| DATA-02 | Po publikacji migracja MUSI pozostać niezmienna. Późniejsza zmiana opublikowanych danych wymaga zwykłej nowej migracji. |
| DATA-03 | Próby migracji MUSZĄ używać baz tymczasowych lub jawnych kopii. Nie wolno resetować realnej bazy użytkownika, usuwać jej danych ani zmieniać globalnej konfiguracji CLI. |
| DATA-04 | Jedna migracja MUSI zachować historię, profile, poświadczenia, layout i obce tabele. Każda rozmowa dostaje nowy `ThreadId` (UUIDv7) i trwały wpis `thread_alias(legacy_host_id, legacy_chat_id) -> thread_id`. Adopcja NIE MOŻE tworzyć drugiej historii tej samej rozmowy. |
| DATA-05 | Model MUSI korzystać z istniejących tabel historii, profili i integracji. Replika wątków innych hostów używa tych samych tabel historii. Nie wolno tworzyć równoległego magazynu transkryptów ani tabel dla niewdrożonych przyszłych funkcji. |
| DATA-06 | Zapis MUSI pozostać wersjonowany. Nowszy lub uszkodzony format jest odrzucany bez zastąpienia go pustymi danymi. |
| DATA-07 | Trwały stan MUSI przechowywać tożsamość i konfigurację, a nie PID, uchwyt procesu lub deklarację, że dawny proces nadal działa. |
| DATA-08 | Wszystkie trwałe zmiany tego samego przejścia domenowego MUSZĄ zatwierdzać się atomowo w SQLite, jeżeli należą do tej bazy. Skutki w filesystemie i u agenta wymagają osobnego stanu wykonania. |
| DATA-09 | Migracja MUSI mieć jawny znacznik wydania i fingerprint niewydanego schematu. DDL, adopcja oraz końcowy znacznik MUSZĄ zatwierdzać się w jednej transakcji. Niezgodny prywatny schemat daje UnsupportedSchema bez zapisu, resetu lub automatycznej naprawy. |

Docelowy podział danych jest następujący. Nazwy tabel są propozycją implementacyjną;
zmiana nazwy przed wydaniem nie zmienia kontraktu danych.

| Dane | Magazyn i istotne pola |
| --- | --- |
| Historia i części rozmowy | Obecne `_canopy_rust_chat_items` i `_canopy_rust_chat_epochs`; klucz `(ThreadId, history_epoch, item)`. Te same tabele przechowują replikę obcych wątków. |
| Lokalny layout, narzędzia i konta | Obecne tabele sesji, katalogu narzędzi, preferencji i integracji. |
| Tożsamość hosta | Nowy singleton: HostId, wersja formatu, trwałe górne granice przydzielonych lokalnych ID. Klucz TLS jest w magazynie systemowym. |
| Mapowanie ID | `thread_alias(legacy_host_id, legacy_chat_id) -> thread_id`. Trwałe; bez usuwania przy Delete wątku. |
| Replikacja | Kursor strumienia katalogu i transkryptu per środowisko źródłowe; HLC i wartości preferencji użytkownika. |
| Katalog wątków i kolejka | Nowe metadane: ThreadId, HostId środowiska, revision, executor_epoch, właściciel checkout/private, AgentDefinitionId, ProfileId, konfiguracja opcji, provider session, history_epoch i checkpoint. Trwałe pozycje kolejki zawierają kolejność, treść/referencje i stan dostarczenia. Pane odwołuje się do ThreadId. |
| Zaufane urządzenia | DeviceId lub principal MCP, publiczna tożsamość credentialu, uprawnienia i zakresy, wersja uprawnień, stan revoked oraz bieżący command_epoch. Bez prywatnego klucza klienta. |
| Inbox operacji | HostId, ThreadId, executor_epoch, DeviceId lub principal, command_id, metoda, cel, typowane argumenty lub referencje, fingerprint, warunki, stan, wynik i czasy. |
| Checkpoint | Wersja i granica zapisanego stanu rozmowy, atomowa z odpowiednimi elementami historii. Nie jest pełnym dziennikiem zdarzeń. |
| Dirty presence | DeviceId, ResourceRef pliku, bazowy hash, dirty, last_seen. Bez tekstu bufora; wpisy obejmują również odłączone urządzenia. |
| Klient | Zaufane hosty i trasy, preferencje urządzenia, cache z last_seen, własne szkice oraz outbox tekstu. Prywatne credentials są w magazynie systemowym klienta. |

Jedna migracja adoptuje aktualne metadane chat pane'ów i zachowanych rozmów do katalogu.
Sprzeczne lub nieczytelne rekordy zostają zachowane z diagnostyką. Nie wolno wybierać
przypadkowego konta ani usuwać historii jako sposobu naprawy. Prywatne formaty etapów
nie wymagają czytników zgodności ani dual write. Testy przed wydaniem ponownie tworzą
bazę z wersji sprzed przedsięwzięcia i stosują końcową migrację dokładnie raz.

Znacznik runtime'u jest sprawdzany niezależnie od starego _migrations Electrona.
Baza Rust v1 oraz jawnie zaimportowana kopia bazy Electrona bez _canopy_rust_meta
przechodzą ten sam pojedynczy krok do wersji runtime v2. Oryginał Electrona pozostaje
tylko do odczytu. Samo CREATE TABLE IF NOT EXISTS nie stanowi wersjonowania migracji.

Build pracujący na niewydanej migracji MUSI wymagać jawnego CANOPY_DATA_DIR poza
domyślnym katalogiem użytkownika. Brak tego ustawienia kończy start przed otwarciem
bazy do zapisu. Rozpoznany wcześniejszy prywatny fingerprint daje diagnostykę
i zachowuje pliki, także gdy zawierają realne dane. Nie tworzymy migracji między
takimi fingerprintami. Nową próbę wykonujemy na nowej kopii źródłowej bazy.
Prywatny build zapisuje znacznik prerelease z fingerprintem zamiast końcowego v2.
Build wydany MUSI odrzucić każdy znacznik prerelease bez zapisu; testuje się także
kopię prywatnej bazy wskazaną przez CANOPY_DATA_DIR. Nie promujemy prywatnych baz
przez podmianę znacznika. Po publikacji obowiązuje stabilny znacznik wydania.
Nie obiecujemy, że już wydany
stary binarny klient rozpozna nowy znacznik: nie wolno współdzielić tej bazy z takim
klientem. Downgrade wymaga zachowanej kopii sprzed migracji, bez nadpisywania nowej bazy.

## 8 Komendy i potwierdzenia wykonania

### 8.1 Koperta i stan

Mutacja zawiera `command_id` UUID, ResourceRef, typowane argumenty oraz warunki potrzebne
danej operacji. Komenda wątku zawiera ThreadRef i `executor_epoch`. Kopertę obsługuje
szyna komend (BUS-02). Tożsamość DeviceId pochodzi z uwierzytelnienia. Dla lokalnego desktopu
istnieje stały principal właściciela hosta. Czyste operacje widoku i odczyty nie trafiają
do inboxu. Lokalny principal nie jest credentialem dostępnym przez gRPC. Wejście PTY
oraz porcje uploadu mają osobne kontrakty, bez kopii bajtów w inboxie.
Zdalna mutacja zawiera również command_epoch urządzenia. Jest to trwała epoka
przyjmowania komend, odrębna od runtime_epoch, runu i historii.

Fingerprint obejmuje metodę, cel, argumenty i warunki. Implementacja porównuje ich
znaczenie po dekodowaniu; nie zakłada kanonicznej serializacji Protobuf. Sekretne pola
nie mogą trafić do jawnego payloadu inboxu. Obecny bezpieczny zapis credentiali ma
przechować sekret w magazynie systemowym i pozostawić w bazie wyłącznie referencję.

Sekretne wejście ma niezmienną wersję w vault, związaną z command_id. Zapis tej wersji
poprzedza commit accepted. Równoległa rotacja profilu nie zmienia zaakceptowanej wersji.
Retry porównuje tę samą treść po normalizacji oraz standardowy MAC sekretu z kluczem
chronionym przez magazyn systemowy. Jawny hash słabego hasła nie wystarcza.
Nie tworzymy własnego protokołu kryptograficznego. Niedostępny vault blokuje przyjęcie
lub dispatch, bez zastąpienia sekretu pustą wartością. Utrata klucza nie pozwala uznać
starego ID za nowe: host odmawia weryfikacji bez ponowienia skutku.

Cleanup może usunąć tylko wersję, do której nie odwołuje się inbox, aktywna konfiguracja
ani trwający zapis. Błąd commitu pozostawia rozpoznawalną wersję do cleanupu; retry
tego samego command_id nie tworzy kolejnego sekretu. Zatwierdzona konfiguracja wskazuje
nowy credential dopiero po sukcesie istniejącego workflow. Nie jest wymagana wspólna
transakcja SQLite i systemowego vault, bo nie zapewniają jej używane magazyny.

| Stan | Znaczenie |
| --- | --- |
| rejected | Host odrzucił polecenie przed skutkiem. Powód może dotyczyć uprawnień, warunku lub formatu. |
| accepted | Host zatwierdził kompletny zamiar w bazie. To nie oznacza przyjęcia przez agenta. |
| attempting | Host zapisał rozpoczęcie próby skutku poza transakcją SQLite. |
| succeeded | Potwierdzono skutek tej operacji. Dla Send może to być przyjęcie do kolejki hosta; tura ma oddzielny status. |
| failed | Host ma konkretny dowód błędu. Wynik określa potwierdzone częściowe skutki, jeśli wystąpiły. |
| delivery_unknown | Host nie może rozstrzygnąć, czy skutek zewnętrzny wystąpił. |

| ID | Wymaganie |
| --- | --- |
| CMD-01 | Wykonawca wątku MUSI deduplikować komendy po `(ThreadId, executor_epoch, DeviceId/principal, command_id)`, także po utracie połączenia, restarcie hosta i dostarczeniu z outboxu innego hosta. Przy przyszłym handoffie rekordy inboxu przechodzą razem z wątkiem. Lokalne operacje używają tej samej szyny; ich retencję określa wyjątek poniżej. |
| CMD-02 | Powtórzenie tej samej komendy w otwartej command_epoch MUSI zwrócić jej dotychczasowy stan. Ten sam command_id z innymi argumentami daje idempotency_conflict. Zamknięta epoka podlega CMD-10. |
| CMD-03 | Host MUSI sprawdzić aktualne uprawnienia przed zwrotem wyniku dedupe i przed dispatch. Identyfikator komendy nie jest uprawnieniem do odczytu. |
| CMD-04 | Host MUSI zatwierdzić accepted i odtwarzalny zamiar przed dispatch. Błąd zapisu zabrania wykonania skutku. |
| CMD-05 | Host MUSI zapisać attempting przed skutkiem zewnętrznym. Awaria po tej granicy daje delivery_unknown, chyba że backend dostarcza wiarygodny dowód wyniku. |
| CMD-06 | Host NIE MOŻE automatycznie powtórzyć delivery_unknown. UI musi zachować treść, wyjaśnić niepewność i umożliwić sprawdzenie stanu. Nowa próba jest jawną nową komendą. |
| CMD-07 | Komenda accepted bez rozpoczętego skutku MUSI po restarcie przejść ponowną autoryzację i walidację. Stara zgoda, Stop, terminal i operacja destrukcyjna nie mogą zostać odtworzone jako nowy skutek. |
| CMD-08 | Komenda zmieniająca tylko bazę MOŻE przejść od razu do succeeded w jednej transakcji. Nie wymaga fikcyjnego attempting. |
| CMD-09 | Pełne zakończone payloady inboxu MOGĄ być kompaktowane po 30 dniach. Minimalny rekord dedupe MUSI pozostać do bezpiecznego zamknięcia command_epoch albo revoke urządzenia. Nie wolno zapomnieć ID i uznać starej próby za nową. |
| CMD-10 | Host MUSI odrzucić mutację z zamkniętej command_epoch jako command_retired przed dispatch. Zamknięcie epoki nie zmienia credentialu ani nie kasuje szkiców. Revoke i ponowne parowanie tworzą nową tożsamość; klient nie przenosi automatycznie do niej starego outboxu. |
| CMD-11 | Inbox MUSI mieć limity rekordów i bajtów per urządzenie, egzekwowane przed accepted. Wyczerpanie limitu urządzenia blokuje tylko jego nowe komendy z resource_exhausted. Odczyt wcześniejszego wyniku i lokalne odzyskanie administracyjne pozostają dostępne. Nie wolno usunąć dedupe po cichu. |
| CMD-12 | Retry mutacji MUSI zachować command_id. Hedging mutacji jest zabroniony. Deadline lub UNAVAILABLE nie są dowodem niewykonania. |

Rekord dedupe po kompaktowaniu zawiera ID, fingerprint i końcowy wynik lub jawny
result_expired. Ten wynik nadal zabrania powtórnego wykonania. Nie jest potrzebny
globalny licznik komend, dziennik wszystkich zdarzeń ani porządek między hostami.

Zakończone lokalne komendy można usuwać bez tombstone po odebraniu wyniku przez
właściciela operacji; nie mają sieciowej ścieżki retry. Stan accepted, attempting
i delivery_unknown oraz trwałe pozycje kolejki pozostają do rozstrzygnięcia.
Niepewne lokalne efekty mają te same reguły jak remote. Lokalny revoke, odczyt stanu i cleanup
zakończonych rekordów muszą działać także przy pełnym inboxie.

CloseCommandEpoch(expected_epoch) jest wąską, idempotentną operacją administracji
własnego inboxu, dostępną również przy limicie. Klient najpierw trwale usuwa z retry
potwierdzone komendy tej epoki i potwierdza odczyt wyników. Host odmawia zamknięcia,
jeśli pozostaje accepted lub attempting. Delivery_unknown pozostaje zapisane do
rozstrzygnięcia, ale nie wykonuje już samo skutku i nie blokuje zamknięcia epoki.
Host sprawdza epokę przed dedupe i dispatch. Atomowo zwiększa epokę i zwalnia
zakończone dedupe poprzedniej. GetCommand nadal może odczytać zachowane unknown
starej epoki; po usunięciu wyniku zwraca command_retired bez ponownego wykonania.
Powtórzenie
Close dla właśnie zamkniętej epoki zwraca jej wynik; starsze żądanie nie zwiększa
epoki ponownie. Opóźniona mutacja starej epoki nigdy nie staje się nową komendą.

Klient uzgadnia bieżącą epokę po reconnect i przed wyczerpaniem budżetu. Niewysłane
teksty offline zachowuje; epokę nadaje przy ich pierwszym faktycznym wysłaniu.
Komendy wcześniej wysłane wymagają odczytu wyniku przed zamknięciem epoki, bez
przenoszenia niepotwierdzonego retry do nowej. Zamknięcie nie usuwa przyjętych
wiadomości ani pozycji kolejki. Nie jest potrzebny licznik każdej komendy ani
lista dziur sekwencji. Ten jeden licznik epoki chroni przed opóźnionym replay po GC.
Not_found nie jest wynikiem wykonania. Przed Close klient ponawia ten sam command_id
w otwartej epoce do uzyskania stanu końcowego albo zachowanego delivery_unknown.
Epoka i outbox mają jednego właściciela na urządzeniu, wspólnego dla jego okien.
Podczas Close klient wstrzymuje tworzenie i wysyłanie nowych kopert, zachowując
nowe intencje jako niewysłane szkice. Wznawia wysyłanie po trwałym uzgodnieniu epoki.
Host rozstrzyga wyścig Close z przyjęciem komendy w tej samej transakcyjnej granicy:
albo komenda blokuje Close, albo zamknięta epoka odrzuca komendę.

Delivery_unknown operacji poza kolejką także można jawnie rozstrzygnąć po sprawdzeniu
stanu. Uprawniony do danej operacji użytkownik zatwierdza archiwizację niepewnego
wyniku. Zapis zachowuje adnotację decyzji użytkownika i nie udaje potwierdzenia skutku
zewnętrznego. Nie wykonuje operacji ponownie. Tak rozstrzygnięty rekord można zwolnić
przy kolejnym Close; osobna nowa próba nadal podlega CMD-06.

Pełne wyniki ponad limit control są artefaktami z ograniczonym okresem przechowania.
Inbox utrwala ich referencje i końcowy status; wygaśnięcie treści nie usuwa dedupe.
PutChunk nie tworzy wpisu inboxu na każdy fragment: offset, hash i stan uploadu
zapewniają jego wąską idempotencję. FinishUpload i operacje publikacji mają command_id.

### 8.2 Wiadomości offline

| ID | Wymaganie |
| --- | --- |
| OFF-01 | Klient MUSI zachować szkic i intencję oczekującą offline w swoim trwałym outboxie. Offline czyta z repliki. Wpis ma ThreadRef, executor_epoch, konto, punkt kontekstu i command_id. |
| OFF-02 | Wpis utworzony offline MUSI czekać na jawne wysłanie po reconnect. Klient pokazuje liczbę wpisów i ich cel. Nie wolno wysłać ich do innego hosta lub rozmowy. |
| OFF-03 | Host MUSI odrzucić outbox dla usuniętego wątku, nieaktualnego executor_epoch lub niezgodnego konta. Zmiana gałęzi rozmowy wymaga ponownego potwierdzenia kontekstu przez użytkownika. |
| OFF-04 | Retry komendy wysłanej wcześniej, której potwierdzenie zaginęło, MUSI użyć starego ID i odpytać stan. Nie jest nowym Send. |
| OFF-05 | Zgody, Steer do dawnego runu, PTY i zapis pliku NIE MOGĄ wykonać się automatycznie po powrocie sieci. Git, Delete i Stop NIE MOGĄ być odtwarzane z outboxu, także jawnie; wymagają nowej komendy po sprawdzeniu stanu. |
| OFF-06 | Załączniki offline MUSZĄ pozostać lokalne do ukończenia uploadu. Anulowanie lub usunięcie rozmowy nie może pozostawić nieograniczonego magazynu fragmentów. |
| OFF-07 | Intencje metadanych wątku MUSZĄ być odtwarzane automatycznie po reconnect z oczekiwaną `revision`. Niezgodna rewizja daje jawny konflikt bez nadpisania zmian właściciela. Send jest odtwarzany tylko jawnie (OFF-02). |

## 9 Kontrakt gRPC i synchronizacji

### 9.1 Usługi

To katalog odpowiedzialności pakietu `canopy.api.v1`. Pakiet jest zbudowany wokół wątku
i komend. Zastępuje niewydany `canopy.remote.v1`, który usuwamy w tym samym refaktorze
([ADR 0001](adr/0001-thread-architecture.md) D14). Implementacja definiuje osobne typowane
metody operacji, a nie `Execute(method, json)`. Nazwy i numery pól `.proto` zostają
utrwalone przy wydaniu. Nie wymagamy jednej długiej sesji bidi dla wszystkich funkcji.

| Usługa | Odczyty i strumienie | Mutacje |
| --- | --- | --- |
| Host | GetHostInfo, GetCapabilities, WatchHost, standardowy Health Check/Watch | Parowanie i zarządzanie Devices przez jawne metody. |
| Catalog | Lista środowisk, projektów, checkoutów, wątków, zasobów i WatchSummaries | Utworzenie/otwarcie projektu, wątku, terminala; rename; preflight i potwierdzenie zamknięcia/usunięcia. |
| Thread | ReadHistoryPage, SearchHistory, SubscribeThread, modele i opcje | Send, kolejka, Steer, Interrupt, decyzje, konfiguracja, restart, fork/edit, goal i operacje tła. |
| Operations | GetCommand, WatchOperation | Jawne CancelOperation tylko dla operacji wspierającej anulowanie. |
| Files i Git | ListDirectory, SearchFiles, ReadFile, WatchResource, diff, historia, refs, plany operacji | CreateFile, SaveFile, zmiany indeksu, commit, Pull/Push, worktree i aktualnie dostępne operacje refs. |
| Terminal | Snapshot i historia ekranu; TerminalSession bidi | AcquireInput, ReleaseInput oraz input/resize z fencing tokenem. |
| Artifacts | DownloadChunks i stan uploadu | BeginUpload, PutChunk, FinishUpload, CancelUpload; obiekty obrazów i stron. |
| Settings i Integrations | Zatwierdzone preferencje bez sekretów, profile i dane Tasks | Typowane zmiany ustawień, kont, filtrów, zadań i powiązań zgodne z aktualnymi modułami. |

`HostInfo` zawiera HostId, wersję aplikacji, wersję kontraktu, limity oraz capabilities.
Klient sprawdza zgodność przed mutacjami. Nowe pola są addytywne; usunięte numery pól
pozostają reserved. Nieznana obowiązkowa operacja daje unsupported, a nie pozorny sukces.
Typy Protobuf należą do granicy transportowej; wspólny model nie musi ich używać.

| ID | Wymaganie |
| --- | --- |
| RPC-01 | Wszystkie kanały zdalne MUSZĄ używać natywnego gRPC/Protobuf. Transport `Local`, adapter gRPC i adapter MCP wywołują wspólną szynę komend. |
| RPC-02 | Host MUSI rozdzielić błąd transportu od wyniku domenowego. Przyjęta operacja ma odpytywalny stan niezależnie od życia RPC. |
| RPC-03 | Cancel RPC MUSI zwalniać zasoby odczytu i subskrypcji. Przyjęta mutacja pozostaje własnością runtime'u do końca lub jawnego CancelOperation. |
| RPC-04 | Odczyty MUSZĄ mieć deadline i ograniczony retry. Mutacje używają CMD-12. Nie wolno ustawić krótkiego deadline'u na życie całej tury agenta. |
| RPC-05 | Health MUSI zgłaszać gotowość usług, keepalive stan połączenia, a WatchHost stan domenowy starting, ready, draining lub storage_fault. Brak pojedynczego agenta nie może oznaczać awarii wszystkich usług. |
| RPC-06 | Klient MUSI obsługiwać zmianę trasy do tego samego HostId. Inna tożsamość endpointu wymaga jawnego ponownego ustanowienia zaufania. |
| RPC-07 | Stos Rust i natywny moduł mobile MUSZĄ obsłużyć unary, server stream, bidi, TLS z pinningiem, mTLS, anulowanie i reconnect na odbieranych platformach. Generowanie typów nie jest dowodem działania transportu. |
| RPC-08 | Kontrakt MUSI być pakietem `canopy.api.v1`. Pakiet `canopy.remote.v1` NIE MOŻE pozostać w repozytorium ani działać równolegle po refaktorze. |
| RPC-09 | Wątek MUSI być adresowany przez `ThreadRef{host_id, thread_id}`, a środowisko przez `EnvironmentRef{host_id}`. `ChatRef` i `WorkspaceRef` NIE MOGĄ występować w API. |
| RPC-10 | Jedna komenda MUSI mieć jedno RPC zwracające `CommandReceipt`. Generyczne `Execute` jest zabronione. |
| RPC-11 | Proto jest pisane ręcznie. Każde RPC komendy MUSI mieć opcję `(canopy.api.v1.command) = {name, risk, scope}`. Test kontraktowy MUSI porównać deskryptor, rejestr komend i listę narzędzi MCP. |
| RPC-12 | CI MUSI uruchamiać `buf lint`. `buf breaking` MUSI blokować zmiany od wydania v1. Zmiana łamiąca wymaga pakietu `canopy.api.v2`. |

### 9.2 Obiekty i transfer

BeginUpload wiąże UploadId z urządzeniem, celem, przeznaczeniem, rozmiarem i hashem
całości. Tych pól nie wolno zmienić po przyjęciu. PutChunk zawiera offset i bajty.
Identyczne ponowienie potwierdzonego fragmentu jest bezpieczne; inne bajty w tym
samym zakresie dają conflict. Niedozwolone nakładanie i wyjście poza rozmiar są odrzucone.
Odczyt stanu pozwala wznowić brakujące zakresy, bez ponownego przyjęcia całej treści.

FinishUpload sprawdza kompletność, integralność, aktualne uprawnienia i istnienie
celu. Publikuje niezmienny ArtifactId raz, pod tą samą blokadą zasobu co Delete.
Niekompletny obiekt nie jest widoczny jako gotowy załącznik. Send akceptuje wyłącznie
gotowe referencje. Akceptowana wiadomość utrzymuje obiekt do zakończenia użycia;
fork zachowuje własne kopie lub referencje i nie traci obrazu przy usunięciu źródła.
Zmiana pliku podczas uploadu jest sprawdzana przez SaveFile przy zastosowaniu
obiektu, a nie tylko w chwili BeginUpload.
Akceptowana operacja inna niż Send również utrzymuje referencję do swoich argumentów
bulk do rozstrzygnięcia; TTL nie usuwa jej spod trwającego dispatch.

Anulowanie, revoke, Delete i wygaśnięcie usuwają niedokończone obiekty kontrolowanym
cleanupem. Restart rozpoznaje osierocone fragmenty i publikuje wynik wcześniejszego
Finish zamiast tworzyć duplikat. Limity obejmują też ukończone niepodpięte obiekty.
Download sprawdza zakres obiektu przy każdym nowym żądaniu; ArtifactId nie jest
uprawnieniem. Media i zależności HTML podlegają tej samej autoryzacji. Ich odczyt
nie może otwierać dowolnej ścieżki hosta. Obrazy zachowują obecną normalizację.

### 9.3 Snapshot i strumień

Cursor składa się z `runtime_epoch`, `stream_id` i rosnącego `sequence`.
`runtime_epoch` zmienia się przy restarcie hosta. Generacja runu służy do odrzucania
wyników dawnego procesu. Rewizja zasobu służy do ochrony konkretnej mutacji.
Te pojęcia nie są zamienne. Send nie wymaga rewizji napływającego transkryptu.

| ID | Wymaganie |
| --- | --- |
| SYNC-01 | Subskrypcja MUSI ustanowić spójną granicę S i zachować tail przed publikacją snapshotu. Klient stosuje kompletny snapshot, potem wyłącznie zmiany większe od S. |
| SYNC-02 | Reconnect MUSI odtworzyć brakujące zmiany z ringa albo wymusić nowy snapshot. Przepełnienie podczas snapshotu unieważnia transfer; częściowego snapshotu nie wolno zastosować. |
| SYNC-03 | Delta MUSI adresować `(ThreadId, history_epoch, ItemId)`. Append zawiera rewizję i offset bajtów UTF-8. Niezgodność wymaga ReplaceItem albo resync, bez podwójnego dopisania. |
| SYNC-04 | Runtime MUSI raz odebrać lokalne take_changes i rozdzielić zdarzenia do niezależnych subskrypcji. Jeden klient nie może opróżnić zmian przeznaczonych dla drugiego. |
| SYNC-05 | Lista podsumowań MUSI działać bez transkryptów. Paginacja historii ma stałą granicę odczytu i stabilne klucze. Usunięcie lub zmiana gałęzi jawnie unieważnia stary page token. |
| SYNC-06 | Live stream NIE MOŻE czekać na checkpoint. Checkpoint MUSI atomowo zapisywać granicę trwałości i odpowiednie elementy. Klient odróżnia live od saved. |
| SYNC-07 | Trwałe zakończenie tury MUSI obejmować końcowe elementy i TurnEnd w jednej transakcji. Błąd tego zapisu nie może dać oznaczenia saved. |
| SYNC-08 | Po awarii niezapisana końcówka MOŻE zniknąć. Klient MUSI zastąpić ją checkpointem z oznaczeniem interrupted. Nie wolno wnioskować, że skutki narzędzi zostały cofnięte. |
| SYNC-09 | Skrócony zapis MUSI zawierać trimmed i informację o niepełnej treści. Offsetu pełnego live tekstu nie wolno zastosować do skróconego checkpointu. Rekord niemożliwy do zapisania po skróceniu MUSI otrzymać ograniczony placeholder o tym samym ID, z powodem utraty treści. Saved opisuje wtedy niepełną reprezentację, nigdy pełny tekst live. |
| SYNC-10 | Wolny klient MUSI dostać resync_required po przekroczeniu limitu. Nie może blokować odczytu agenta, innych klientów ani control RPC. |

Runtime może zachować istniejący batching historii 500 ms i wcześniejszy zapis końca
tury. Potwierdzenie accepted oraz finalizacja potrzebują rzeczywistego wyniku commitu;
samo ustawienie flagi urgent nie wystarcza. Nie jest wymagany trwały zapis każdego tokenu.
Bez rekordu lub placeholdera nie wolno przesunąć granicy saved. Trwale zapisany
placeholder umożliwia zapis dalszych elementów i TurnEnd oraz poprawne Quit;
utrata treści nie może powodować bezterminowego ponawiania niepoprawnego rekordu.
Resync jest stanem przejściowym. Po ustaniu błędu lub nadmiarowego ruchu klient MUSI
zakończyć snapshot i zastosować następną poprawną deltę w ciągu 10 s na stanowisku
AC-30. Duże snapshoty są strumieniowane porcjami i nie omijają limitów pamięci.

### 9.4 Replikacja między hostami

Model replikacji „A+” opisuje [ADR 0001](adr/0001-thread-architecture.md) D4–D6.
Każde środowisko jest jedynym zapisującym dla swoich wątków. Nie ma porządku
globalnego między hostami.

| ID | Wymaganie |
| --- | --- |
| REPL-01 | Środowisko wątku MUSI być jedynym zapisującym dla wątku i jego metadanych. Inny host NIE MOŻE zmienić repliki lokalnie; wysyła intencję przez outbox i inbox. |
| REPL-02 | Rekord wątku MUSI mieć `revision`. Intencja zmiany metadanych zawiera oczekiwaną rewizję; niezgodność daje jawny konflikt. |
| REPL-03 | Katalog MUSI być strumieniem per środowisko z kursorem i snapshotem. Brak wątku w snapshocie oznacza jego usunięcie u odbiorcy. |
| REPL-04 | Sparowane hosty MUSZĄ łączyć się bezpośrednio. Relay przez inny host NIE jest dozwolony. Transkrypt synchronizuje się od hosta wykonawczego. |
| REPL-05 | Każdy sparowany host MUSI mieć replikę wszystkich wątków, pełnego transkryptu i artefaktów. Replika poza właścicielem jest tylko do odczytu. |
| REPL-06 | HLC z LWW MOŻE rozstrzygać wyłącznie preferencje użytkownika bez właściciela. NIE MOŻE rozstrzygać metadanych wątku. |
| REPL-07 | Niedostępny host źródłowy NIE MOŻE blokować odczytu jego repliki ani pracy z innymi hostami. Replika ma last_seen i oznaczenie nieaktualności (LIFE-08). |

## 10 Pliki Git integracje i ustawienia

| ID | Wymaganie |
| --- | --- |
| APP-01 | Odczyt drzewa, wyszukiwanie, tworzenie i zapis MUSZĄ działać na wybranym hoście. Zapytanie zawsze wskazuje root zasobu; klient nie przekazuje dowolnej ścieżki do wykonania poza swoim zakresem. |
| APP-02 | Edytor MUSI zachować BOM, CRLF i uprawnienia oraz obecne limity. Zapis wymaga zgodności bazowej treści i tożsamości celu z dyskiem hosta. Konflikt zachowuje dirty buffer. Nie wolno dodać cichego force overwrite. |
| APP-03 | Dwa urządzenia MOGĄ edytować plik niezależnie. Pierwszy poprawny zapis zmienia wersję bazową. Zapis drugiego dostaje conflict i zachowuje jego treść. Współedycja znak po znaku nie jest częścią wydania. |
| APP-04 | Klient MUSI zgłaszać presence dirty buffer bez przesyłania tekstu. Preflight zamknięcia/usunięcia MUSI uwzględnić online dirty clients i ostatni znany stan offline clients. Nie wolno zdalnie odrzucić cudzego aktywnego bufora. |
| APP-05 | Po utracie klienta jego lokalny draft MUSI pozostać na urządzeniu. Potwierdzone usunięcie zasobu hosta nie usuwa tego drafta; ponowny zapis wymaga jawnego nowego celu. |
| APP-06 | Mutacje Git MUSZĄ używać libgit2 i obecnych usług. Warunki obejmują właściwe dla operacji HEAD, indeks, stan worktree i zakres preflight. Niezależne repozytoria nie mogą blokować się globalnym lockiem. |
| APP-07 | Worktree, Pull/Push, commit, podpis i hooki MUSZĄ zachować aktualne reguły analizy, zgód, częściowego sukcesu i cleanupu. Retry wyniku niepewnego nie może tworzyć drugiego commita lub worktree. |
| APP-08 | Tasks MUSZĄ wykonywać istniejące operacje przez konta hosta. Klient nie otrzymuje tokenu konta. Filtry, paginacja, załączniki i kontekst task do worktree zachowują aktualny zakres i walidację. |
| APP-09 | Ustawienia MUSZĄ należeć do jednej z trzech klas. Ustawienia hosta (narzędzia, profile, integracje) zostają na hoście. Ustawienia urządzenia są lokalne; zmiana na jednym urządzeniu nie przestawia drugiego. Preferencje użytkownika synchronizują się między hostami przez HLC z LWW (REPL-06). |
| APP-10 | Zdalny zapis lub usunięcie sekretu MUSI być write-only i wymagać admin. Błąd zapisu konfiguracji zachowuje poprzedni credential i ponawialny cleanup zgodnie z obecnym magazynem. |
| APP-11 | Operacja wymagająca promptu OS na hoście MUSI zwrócić needs_host_interaction z dalszą akcją. Nie wolno zawiesić RPC w oczekiwaniu na niewidoczny picker, pinentry, Keychain prompt lub OAuth. |
| APP-12 | Otwarcie linku, schowek, wybór lokalnego pliku i focus MUSZĄ działać na urządzeniu inicjującym. Wybór pliku hosta jest zapytaniem o jego katalog, a nie natywnym pickerem otwieranym na hoście. |
| APP-13 | Media i strony MUSZĄ być udostępniane jako autoryzowane artefakty lub zakresy odczytu. Bezwzględna ścieżka hosta nie może stać się lokalnym file URL klienta. |
| APP-14 | Strony HTML MUSZĄ zachować izolację obecnego podglądu. Kod strony nie może używać credentiali remote ani wywoływać RPC Canopy. preview_html wykonuje host, jeśli ma renderer; brak renderera daje jawne unsupported. |
| APP-15 | Zapis notes, operacje zasobów i prezentacja diffu MUSZĄ zachować aktualne zachowanie, błędy i ograniczenia. Istniejąca platformowo niedostępna funkcja nie może otrzymać fikcyjnego sukcesu z remote. |

Presence dirty buffer jest informacją o kliencie, a nie blokadą distributed filesystemu.
Po odłączeniu staje się stale. Usunięcie z potwierdzeniem ryzyka offline draft wymaga
uprawnienia do usuwania zasobu. Online właściciel bufora musi wcześniej zapisać go,
odrzucić lub anulować operację. Host nie pobiera cudzego tekstu bez tej operacji.
Presence jest zapisana przed potwierdzeniem klientowi i przetrwa restart hosta.
Wpis znika po potwierdzonym save, discard, revoke albo jawnym Dismiss na hoście.
Dismiss ujawnia ryzyko i nie odrzuca drafta urządzenia. Stare wpisy nie znikają
wyłącznie z powodu upływu czasu; host pokazuje last_seen i pozwala je rozstrzygnąć.
Błąd zapisu z powodu dirty wskazuje urządzenie i last_seen, także gdy zgłasza go ACP.
Nie wymaga ani nie ujawnia tekstu bufora.

Blokady Canopy chronią równoległe operacje Canopy. Nie blokują agenta, edytora zewnętrznego
ani innego procesu systemu. Obecne sprawdzenie treści przed atomowym replace ma końcowe
okno wyścigu. Wydanie zachowuje tę granicę i wykrywa dostępne rozbieżności; nie deklaruje
uniwersalnego CAS filesystemu. Wykryty konflikt zachowuje treść i nie uruchamia zapisu ponownie.

Przypisanie każdego ustawienia do klasy APP-09 jest otwartą kwestią
[ADR 0001](adr/0001-thread-architecture.md).

Logowanie wymagające hosta można wykonać na hoście i ponowić operację z klienta.
Nie przenosimy systemowych promptów na telefon. To ograniczenie interakcji OS,
a nie brak implementacji zwykłego zarządzania kontami przez remote.

## 11 Terminal i shell

Host utrzymuje istniejący emulator Alacritty. Lokalny i zdalny renderer czytają jego
projekcję ekranu. Remote nie musi odtwarzać prywatnego stanu parsera ANSI we własnym
emulatorze. Snapshot zawiera komórki, style, Unicode, cursor, tryby wejścia, rozmiar,
identyfikator runu i sequence. Historia ekranu jest ograniczona i stronicowana.
Kolejne zmiany są zmianami tej projekcji. Nie są przypadkowym wycinkiem bajtów PTY.

| ID | Wymaganie |
| --- | --- |
| TERM-01 | Terminal MUSI uruchamiać proces tylko na hoście i korzystać z obecnej konfiguracji środowiska, grup procesów i Windows Job Object. |
| TERM-02 | Terminal użytkownika MUSI mieć jednego właściciela input i resize. Utworzenie przyznaje własność inicjującemu klientowi; przejęcie wymaga jawnego działania i zwiększa fencing token. Terminal ACP podlega AGT-10 i nie przyznaje wejścia człowiekowi. |
| TERM-03 | Każdy input i resize MUSI wskazywać run i fencing token. Nieaktualny token lub obserwator dostaje odmowę bez zapisania bajtów. |
| TERM-04 | Właściciel MUSI odnawiać dzierżawę co 5 s. Po 15 s bez odnowienia lub po potwierdzonym odłączeniu host ją zwalnia. Proces pozostaje uruchomiony. |
| TERM-05 | Potwierdzenie input MUSI oznaczać queued w ograniczonej kolejce hosta. Nie może oznaczać wykonania przez shell. Klient nie ponawia automatycznie bajtów po utracie ACK. |
| TERM-06 | Reconnect MUSI dać spójną projekcję i dalsze zmiany od jej granicy. Alternate screen, tryby wejścia i szerokie znaki muszą być zgodne z hoście. Utracony scrollback jest jawnie oznaczony. |
| TERM-07 | Scroll i zaznaczenie MUSZĄ być per view. Scroll telefonu nie przewija desktopu. Bracketed paste i klawisze sterujące zachowują tryby aktualnego runu. |
| TERM-08 | Restart, Stop i usunięcie MUSZĄ dołączać do wspólnego bounded cleanupu. Nie wolno zwolnić worktree lub prywatnych plików przed potwierdzonym końcem zarządzanych procesów. |
| TERM-09 | Shell z composera MUSI działać w cwd i środowisku hosta. Zachowuje obecne ograniczenia outputu, raport do agenta, Stop i cleanup rozmowy. |

Możliwość przejęcia wejścia jest ograniczona uprawnieniem terminal. Lokalny użytkownik
hosta może jawnie odzyskać własność. Nie ma automatycznego przejmowania przy focusie.
Wolny terminal subscriber podlega tym samym limitom co inne widoki i nie blokuje parsera.

## 12 Połączenia uprawnienia i sekrety

Host ma przypiętą tożsamość TLS. Pierwsze parowanie odbywa się przez krótkotrwały
jednorazowy kod lub QR z fingerprintem hosta. Użytkownik hosta zatwierdza urządzenie
i jego zakres. Sparowane połączenia używają standardowego TLS z poświadczeniem klienta
oraz sprawdzeniem uprawnień w Canopy. Nie budujemy zewnętrznej PKI.

| Zakres | Uprawnienia |
| --- | --- |
| read | Odczyt dozwolonych zasobów, transkryptów, plików, Tasks i statusów. |
| chat | Tworzenie i prowadzenie rozmów, kolejka, konfiguracja rozmowy i Interrupt. |
| approve | Odpowiadanie na decyzje agenta, plan i pytania. Nie wynika automatycznie z read. |
| files_git | Zapis plików i istniejące mutacje Git w nadanych projektach. |
| terminal | Uruchomienie terminala, input, resize, przejęcie i Stop w swoim zakresie. |
| tasks | Istniejące zapisy Tasks z uprawnieniami konta hosta. |
| admin | Profile, konta, sekrety, zaufane urządzenia i konfiguracja Remote. |

Delete i operacje worktree wymagają ponadto odpowiedniego uprawnienia do zasobu
oraz ważnego potwierdzenia skutków. Admin nie omija ograniczeń filesystemu,
backendu ani serwisów dostawcy. Zgodnie z aktualną decyzją produktu domyślna zgoda
udostępnia wszystkie zakresy i wszystkie obecne oraz przyszłe zasoby (`*`).
Użytkownik może ograniczyć operacje lub projekty przy zatwierdzaniu i później.

| ID | Wymaganie |
| --- | --- |
| SEC-01 | Wszystkie zwykłe RPC MUSZĄ weryfikować tożsamość hosta i credential klienta. Jedynym wyjątkiem od istniejącego credentialu klienta jest ograniczony enrollment opisany poniżej. SSH i VPN nie zastępują autoryzacji Canopy. TLS obowiązuje także na LAN. |
| SEC-02 | Pierwszy enrollment MUSI być wyłączony poza jawnym oknem 5 minut. Token ma jednorazowe użycie i minimum 128 bitów losowości z systemowego generatora. Nazwa lub deviceId nie są dowodem tożsamości. |
| SEC-03 | Nieautoryzowany klient NIE MOŻE odczytać katalogu, health szczegółowych usług ani transkryptu. Endpoint parowania ujawnia tylko dane potrzebne do ustanowienia zaufania. |
| SEC-04 | Zmiana zakresu lub revoke MUSI działać również na istniejących streamach. W ciągu 1 s od zatwierdzenia host zamyka niedozwolone streamy, zwalnia PTY lease i odrzuca jeszcze niewykonane komendy. |
| SEC-05 | Revoke NIE MOŻE być raportowane jako cofnięcie już rozpoczętego efektu. Pracujący agent pozostaje zasobem hosta; admin może go osobno zatrzymać. |
| SEC-06 | Snapshot konfiguracji, diagnostyka i zwykły inbox NIE MOGĄ ujawniać tokenów, prywatnych kluczy ani credentiali konfiguracji. Sekrety dostawcy pozostają na hoście, a credentials klienta w jego magazynie systemowym. |
| SEC-07 | Credential write MUSI mieć kontrolowany cleanup przy częściowym błędzie zapisu. Nie wolno traktować sekretu jak zwykłego pola JSON lub pozwolić na jego odczyt zwrotny. |
| SEC-08 | Host MUSI sprawdzać zakres ResourceRef, kanoniczny root i reguły symlink/reparse przed I/O. Nie wolno polegać na tym, że UI ukryło niedozwoloną akcję. |
| SEC-09 | Bez włączonego udostępniania listener pozostaje wyłączony lub na loopback. Dodanie komputera jest świadomym włączeniem udostępniania przez LAN/VPN, także dla połączenia zwrotnego. Zmiana listenera nie może zmienić HostId ani automatycznie zaufać innemu endpointowi. |
| SEC-10 | Jedno zatwierdzenie połączenia desktop MUSI ustanowić oba kierunki bez drugiego QR. Zaufanie zwrotne wymaga uwierzytelnionego RPC z admin, dopasowania CSR do przypiętej tożsamości hosta i trwałego zapisu credentialu. Ponowienie nie tworzy duplikatów, nie przywraca odwołanej zgody ani ręcznie wstrzymanego połączenia. |

Pozycja kolejki utworzona przez zakończony Send jest stanem rozmowy hosta.
Revoke nadawcy nie usuwa tej pozycji ani nie cofa przyjętej wiadomości. Host może
nią zarządzać według CHAT-02. Revoke zatrzymuje nowe i jeszcze nieprzyjęte zamiary
urządzenia; uprawnienia agenta i bezpieczeństwo narzędzi nadal są sprawdzane.

Pierwsze parowanie używa ograniczonego endpointu enrollment przez TLS z fingerprintem
hosta z kodu/QR. Tylko ten endpoint dopuszcza klienta bez wcześniejszego certyfikatu.
Żądanie wiąże jednorazowy token z publicznym kluczem urządzenia i dowodem posiadania
odpowiadającego klucza prywatnego. Lokalna zgoda na hoście wskazuje ten sam klucz
i nadawany zakres. Standardowy mechanizm mTLS wydaje lub rejestruje credential
dla tego klucza. Podmiana klucza wymaga nowego parowania. Równoległe użycie tokenu
może zatwierdzić tylko jedno urządzenie. Do zatwierdzenia nie ma dostępu do danych.
Implementacja używa gotowych bibliotek TLS i standardowego dowodu posiadania klucza.
Zmiana zaufanej tożsamości wymaga jawnego ponownego parowania, nigdy wyłączenia TLS.

Canopy na każdym kliencie, także mobile, tworzy i posiada tunel SSH. Konfiguracja
zawiera adres, port, użytkownika, przypięty klucz hosta SSH i referencję do credentialu.
Nie jest dowolnym poleceniem shell. Tunel nie zmienia tożsamości TLS Canopy.
Nowy lub zmieniony klucz SSH wymaga jawnej weryfikacji; nie wolno wyłączyć kontroli
klucza ani zmienić globalnego known_hosts. Nie włączamy agent forwarding.
Disconnect klienta zamyka jego tunel, a reconnect odtwarza go z ograniczonym backoffem.
Telefon nie wymaga tunelu na pomocniczym komputerze. Tailscale/VPN zapewnia istniejącą
trasę systemu; zarządzanie kontem VPN nie jest częścią Canopy.

Konfiguracja remote ujawnia nazwy pól env, ale wartości są write-only i mają akcję
LeaveUnchanged. Nie wysyła istniejącego sekretnego override jako JSON. Dawne lokalne
rekordy env pozostają czytelne bez masowego przepisywania; nie są eksportowane do
snapshotu klienta. Uprawniony odczyt pliku, transkryptu lub terminala może zawierać
sekret wpisany przez użytkownika. Nie obiecujemy filtrowania dowolnej treści.
Diagnostyka domyślnie nie zapisuje promptów, plików ani bajtów terminala.

### 12.1 MCP

MCP jest adapterem szyny komend. Model decyzji opisuje
[ADR 0001](adr/0001-thread-architecture.md) D2, D10 i D11.

| ID | Wymaganie |
| --- | --- |
| MCP-01 | MCP MUSI działać na poziomie wątków, niezależnie od miejsca ich wykonania. Odpowiedź ujawnia środowisko i wszystkie metadane wątku. |
| MCP-02 | MCP MUSI udostępniać każdą operację dostępną użytkownikowi w UI, także konfigurację aplikacji (BUS-05). |
| MCP-03 | Każdy principal MCP MUSI mieć osobną tożsamość urządzenia na każdym hoście. Revoke jednego principala nie zmienia innych. |
| MCP-04 | Uprawnienia MUSZĄ pochodzić z grantu OAuth połączenia: Allow, Ask albo Deny dla każdego zakresu. Pełny admin działa bez potwierdzeń. Ask wymaga potwierdzenia, także dla konfiguracji wykonującej kod. |
| MCP-05 | MCP MUSI widzieć wątki wszystkich sparowanych hostów w zakresie grantu. MCP NIE MOŻE adresować okien; akcja widoku zmienia stan globalny (OWN-10). |

Mobile korzysta z natywnego modułu gRPC, a nie z grpc-web. Dobór biblioteki i standardowej
implementacji poświadczeń musi przejść test RPC-07 przed związaniem reszty implementacji
z daną biblioteką. Nie jest wymagana obsługa SSH w czystym JavaScript.

## 13 Limity wydajność i obserwowalność

Poniższe wartości są początkowym kontraktem odbioru. Przed wydaniem można je zmienić
na podstawie pomiaru, razem z testem i uzasadnieniem. Po wydaniu host ogłasza rzeczywiste
limity w capabilities. Nie są to wyniki pomiarów obecnej aplikacji.

| Zasób | Limit lub cel |
| --- | --- |
| Komunikat control | 1 MiB po dekodowaniu. Duży obiekt korzysta z Artifacts. |
| Fragment tekstu lub bulk | 64 KiB; tekst nie kończy się wewnątrz kodowania znaku UTF-8. |
| Strona historii | Do 100 elementów i 1 MiB; większy element jest odczytywany porcjami. |
| Ring zmian | 16 MiB łącznie na host. Przekroczenie usuwa najstarsze zmiany i wymaga resync ich odbiorców. |
| Kolejka wyjścia klienta | 4 MiB łącznie, osobny limit globalny 32 MiB dla fanout. Control ma priorytet, bulk osobne porcje. |
| Odbierani klienci i streamy | Co najmniej 8 klientów i 32 subskrypcje równocześnie w teście obciążenia. |
| Batching live tekstu | Pierwszy fragment od razu; kolejne maksymalnie 30 ms oczekiwania przed oddaniem do transportu, z limitem bajtów. |
| Upload obrazu | Istniejący limit źródła 64 MiB, do 10 obrazów na wiadomość, istniejąca normalizacja przed wysłaniem do modelu. |
| Upload tymczasowy | Maksymalnie 4 równocześnie i 256 MiB łącznie; nieukończone wygasają po 1 godzinie bez aktywności. |
| Outbox klienta | 100 wiadomości lub 64 MiB tekstu i metadanych oraz 256 MiB lokalnych załączników. Przekroczenie zachowuje szkic i pokazuje odmowę. |
| Inbox remote | Do 100 000 rekordów i 64 MiB payloadów, wyników i tombstones na urządzenie; maksymalnie 32 ważne sparowane tożsamości. Revoked nie zajmuje miejsca w limicie parowania. CloseCommandEpoch odzyskuje budżet bez ponownego parowania. Budżet obejmuje referencje i indeksy danych aplikacji. |
| Inbox lokalny | 256 MiB; zakończone i odebrane wyniki są zwalniane, a nierozstrzygnięte pozostają. Lokalny odczyt, revoke i cleanup nie wymagają wolnego wpisu inboxu. |
| Artefakty wyników | Maksymalnie 256 MiB niepodpiętych obiektów na host, wliczając ukończone uploady i duże wyniki inboxu; TTL 1 godzina bez użycia. Podpięte obrazy rozmów podlegają istniejącemu przechowywaniu historii i cleanupowi Delete. |
| Procesy czatu | Maksymalnie 32 równocześnie; uśpienie według LIFE-09 nie uruchamia wstrzymanej kolejki. |
| Terminal | Zachowane maksymalnie 64 PTY i istniejący scrollback 10 000 linii. |
| Przyjęcie komendy | p95 do 100 ms na LAN przy RTT do 10 ms; do 300 ms przy RTT do 100 ms i obciążeniu z macierzy. |
| Przetwarzanie delty | p95 host parse do enqueue do 30 ms; klient receive do apply do 50 ms. |

| ID | Wymaganie |
| --- | --- |
| PERF-01 | Host MUSI egzekwować ogłoszone limity przed nieograniczonym buforowaniem. Brak miejsca daje resource_exhausted z możliwością odzyskania, bez utraty wcześniejszego poprawnego stanu. |
| PERF-02 | Pomiary MUSZĄ rozdzielać kolejkę, commit, dispatch, czas modelu, fanout i apply/render. Nie wolno odejmować timestampów zegarów różnych komputerów bez znanego modelu synchronizacji. |
| PERF-03 | Logi MUSZĄ identyfikować command_id, host, zasób, przejścia stanów, czasy, overflow i przyczyny odmowy. Domyślnie nie zawierają promptów ani wyników narzędzi. |
| PERF-04 | Test obciążenia MUSI objąć wolnego klienta, równoległy bulk i kilka aktywnych sesji. Sam pomiar pustego kanału lub jednego echo RPC nie wystarcza. |
| PERF-05 | Brak użytkownika lub subskrybenta NIE MOŻE powodować ciągłego odrysowywania. Istniejące przyrostowe aktualizacje, wirtualizacja i Reduce Motion muszą pozostać zachowane. |

## 14 Platformy i granice interakcji

Odbiór natywny obejmuje desktop macOS Apple Silicon i Windows 11 x64 oraz mobile iOS
i Android. Są to obowiązkowe wiersze macierzy. Nie wolno uznać cross-compilation za
odbiór Windows lub emulatora za pełny odbiór fizycznego telefonu. Dotychczasowe działające
ścieżki Linux nie mogą regresować; deklaracja nowego produkcyjnego portu Linux nie jest
częścią tego refactoru. Dodanie Linux do deklarowanych hostów wymaga tej samej macierzy.

Ograniczenia stałe tego wydania:

- Headless i praca po Quit hosta nie są dostępne.
- Brak centralnej infrastruktury push. Nie gwarantujemy powiadomienia telefonu przy
  uśpionej lub zakończonej aplikacji. Foreground i dozwolone okno tła synchronizują stan.
- Interakcje OS hosta podlegają APP-11. Nie transmitujemy promptów systemowych.
- Funkcja backendu niewspierana przez jego protokół podlega AGT-07.
- Brak współedycji znakowej i automatycznej migracji wykonania między hostami.
  Jawny handoff wątku jest późniejszym etapem (OWN-09).
- Telefon ma jeden widok aplikacji z własną nawigacją. Wiele okien desktopu ma na
  telefonie N/A-platform; niezależność jego nawigacji i szkiców nadal jest wymagana.

Przeglądarka osadzona jako renderer strony lub terminala nie jest klientem web.
Implementacja musi zachować separację między treścią takiego renderera a credentials RPC.
Niedostępny renderer platformowy ma ten sam jawny stan lokalnie i zdalnie.

## 15 Etapy jednego przedsięwzięcia

Zmiany pozostają niewydane do końcowego odbioru. Nie publikujemy pośredniego protokołu
ani nie wymagamy zgodności z prywatnymi kompilacjami etapów. Etapy mogą być dzielone
na mniejsze zadania, ale nie mogą usuwać funkcji z zakresu SCOPE-01.

| Etap | Wynik pracy | Bramka wyjścia |
| --- | --- | --- |
| E0 Kontrakt i wykonalność | Inwentaryzacja funkcji; rejestr komend; szkic końcowej migracji i `canopy.api.v1`; spike natywnego gRPC Rust oraz mobile z TLS, streamami, anulowaniem i własnym tunelem SSH. | Istnieje działający dowód RPC-07 i tras na obu mobile OS; brak otwartych decyzji zmieniających model tożsamości, uprawnień lub trwałości. Spike nie jest integracją produktu. |
| E1 Własność i dane | Model wątku z `ThreadId` i `thread_alias`; katalog wątków i zasobów; runtime poza widokiem; szyna komend; inbox i jedna aktualizowana migracja. Refaktor wątku i komend kończy się przed pierwszym wydaniem Remote. | Zachowane obecne testy funkcji i cleanupu; operacja działa bez ChatView i Window; migracja zachowuje starą bazę. |
| E2 Backend agentów | Wspólny model, native Claude/Codex, generic ACP, dynamiczne opcje, neutralne narzędzia i wszystkie dotychczasowe rozszerzenia. | Nagrania obu native i kontrakt ACP przechodzą; drugi AgentDefinition ACP nie wymaga zmian w UI ani rdzeniu. |
| E3 Remote i klient desktop | gRPC, pairing, auth, health, dwa hosty, replikacja między hostami, MCP, katalog, komendy, strumienie, replay, artifacts i cały czat. | Zaliczone scenariusze awarii, reconnect i multi-host dla desktopu; frontend nie wymaga lokalnego aktywowania zasobu. |
| E4 Pozostałe funkcje | Files/editor, pełny Git/worktree, terminal, Tasks, settings, profile, sekrety i media. | Każdy wiersz sekcji 3 ma kompletną obsługę domenową, błędy i cleanup; brak makiet zamiast operacji. |
| E5 Mobile | Natywny transport i wszystkie wiersze zakresu w interfejsie mobilnym, outbox, suspend i reconnect. | Odbiór iOS/Android, dotyk, klawiatura, terminal, konflikt pliku i pełne Tasks/settings. Stara ścieżka WebRTC nie jest aktywna. |
| E6 Odbiór wydania | Końcowa migracja, dokumentacja, testy nasycenia, natywne kombinacje urządzeń i przegląd złożoności. | Wszystkie wymagane przypadki mają PASS i dowód; brak nieuzasadnionych warstw; dokładnie jedna migracja tej funkcjonalności. |

Przegląd każdej zmiany ocenia najpierw zachowanie, zachowanie danych oraz ryzyko awarii.
Następnie sprawdza SIMPLE-01 do SIMPLE-07. Ocena liczby plików lub zgodności z wzorcem
architektonicznym nie zastępuje tych pytań.

## 16 Odbiór końcowy

Odbiór jest opisany w [osobnej macierzy](agent-runtime-remote-acceptance.md).
Brak wykonania testu daje NOT RUN. Błąd daje FAIL. N/A wymaga wskazania stałego
ograniczenia z sekcji 14 lub konkretnego protokołu. N/A nie służy do zamykania
niedokończonych funkcji mobile, Windows ani integracji.

GUI/E2E, realne usługi, Keychain i modele uruchamia się według [docs/testing.md](testing.md).
Wymagane wcześniejsze zgody na te operacje pozostają w mocy. Oczekiwanie na taką zgodę
nie może zostać zapisane jako PASS i nie zmienia wymagań wydania.

## 17 Mapa kodu i źródła

Punkty wejścia zweryfikowane przy przygotowaniu planu na `rust-rewrite`, baza `8bef66e`:

- [ChatSession](../src/chat/session.rs), [native adaptery](../src/chat/codex/adapter.rs)
  i [Provider](../src/chat/provider.rs): obecne granice protokołu do rozdzielenia.
- [ChatView](../src/ui/chat/view.rs), [Chats](../src/app_state/chats.rs) i
  [cleanup](../src/app_state/chats/cleanup.rs): obecna własność start/pump/close i guardów.
- [transkrypt](../src/chat/transcript.rs), [historia](../src/chat/history.rs),
  [writer historii](../src/app_state/chat_history.rs) i
  [SQLite historii](../src/settings/database/chat_history.rs): istniejący zapis do zachowania.
- [workspace](../src/state/workspace.rs), [sesja](../src/state/session.rs) i
  [ProjectsState](../src/app_state/projects.rs): tożsamość i lokalny layout.
- [GitService](../src/git/service.rs), [pliki](../src/files.rs),
  [terminal](../src/terminal/session.rs), [integracje](../src/app_state/integrations.rs)
  i [ToolsState](../src/app_state/tools.rs): istniejące operacje do współdzielenia.
- [mobile](../mobile/package.json) i
  [stary snapshot remote](../mobile/src/lib/remote/protocol/state-snapshot.ts):
  obecny klient do przełączenia z mirroringu na kontrakt domenowy.

Dokumentacja procedur: [chat](chat.md), [persist](persistence.md),
[terminal](terminal.md), [Files/editor](files-editor.md), [Git worktree](git-worktrees.md),
[Git Changes](git-changes.md), [Git network](git-network.md), [integracje](integrations.md),
[settings](settings.md), [Windows](windows.md). Historyczny
[remote Electrona](features/remote-control.md) nie jest protokołem docelowym.

Źródła kontraktów zewnętrznych:

- [gRPC lifecycle](https://grpc.io/docs/what-is-grpc/core-concepts/),
  [anulowanie](https://grpc.io/docs/guides/cancellation/),
  [retry](https://grpc.io/docs/guides/retry/) i
  [flow control](https://grpc.io/docs/guides/flow-control/) określają zachowanie transportu;
  nie zapewniają replay i idempotencji domenowej Canopy.
- [Health](https://grpc.io/docs/guides/health-checking/) i
  [keepalive](https://grpc.io/docs/guides/keepalive/) dotyczą różnych poziomów dostępności.
- [Protobuf](https://protobuf.dev/programming-guides/proto3/) określa kompatybilność
  numerów pól, oneof i ewolucji komunikatów. Fingerprint nie może zakładać kanonicznych bajtów.
- ACP: [inicjalizacja](https://agentclientprotocol.com/protocol/v1/initialization),
  [narzędzia i decyzje](https://agentclientprotocol.com/protocol/v1/tool-calls),
  [konfiguracja](https://agentclientprotocol.com/protocol/v1/session-config-options),
  [pliki](https://agentclientprotocol.com/protocol/v1/file-system),
  [terminale](https://agentclientprotocol.com/protocol/v1/terminals) i
  [load/resume](https://agentclientprotocol.com/protocol/v1/session-setup).
  [OpenCode ACP](https://opencode.ai/docs/acp/) jest pierwszym rzeczywistym agentem
  odniesienia. Inne agenty wymagają sprawdzenia wersji i ogłoszonych możliwości.
