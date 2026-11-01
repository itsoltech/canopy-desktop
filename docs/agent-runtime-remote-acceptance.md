# Odbiór abstrakcji agentów i zdalnego sterowania

Ten dokument jest częścią [planu domenowego](agent-runtime-remote-plan.md).
Opisuje testy docelowe. Nie stanowi raportu wykonania testów.
Zmieniony 2026-11-01 zgodnie z [ADR 0001](adr/0001-thread-architecture.md).

## 1 Reguły odbioru i dowody

Wynik przypadku to PASS, FAIL, NOT RUN lub N/A. PASS wymaga wskazania rewizji kodu,
platformy, wersji backendów, wykonanych kroków i wyniku. N/A wymaga konkretnej
capability lub stałego ograniczenia platformowego z planu. Niewdrożona funkcja daje
FAIL lub NOT RUN. Zaliczenie scenariusza na mocku nie dowodzi działania całej integracji.

Każda komórka macierzy funkcjonalnej z sekcji 3 planu ma osobny wynik dla lokalnego
desktopu, remote desktop i remote mobile. Dla właściwych wierszy dochodzą warianty
Claude, Codex i ACP. Wszystkie funkcje wydania muszą być odebrane przed publikacją.

Poniższa tabela wskazuje obowiązkowe przypadki dla każdej komórki. Skróty klientów:
L = lokalny desktop, RM = remote macOS, RW = remote Windows, I = iOS, A = Android.
W kolumnie Backend: C = Claude, X = Codex, P = ACP, — = funkcja niezależna od backendu.
Wiersz z C/X/P tworzy trzy osobne wyniki w każdej komórce, np. F-03-I-P.
L ma osobny wynik macOS i Windows; regresję Linux obejmuje AC-33.

| Zakres | L | RM | RW | I | A | Backend |
| --- | --- | --- | --- | --- | --- | --- |
| F-01 | AC-04,27,28 | AC-04,27,28,31 | AC-04,27,28,31 | AC-04,27,28,31 | AC-04,27,28,31 | — |
| F-02 | AC-03,04,24,37 | AC-03,04,24,31,37 | AC-03,04,24,31,37 | AC-03,04,24,31,37 | AC-03,04,24,31,37 | — |
| F-03 | AC-07,11,12,18 | AC-07,11,12,18 | AC-07,11,12,18 | AC-07,11,12,14,18 | AC-07,11,12,14,18 | C/X/P |
| F-04 | AC-05,06 | AC-05,06 | AC-05,06 | AC-05,06,31 | AC-05,06,31 | C/X/P |
| F-05 | AC-08 | AC-08 | AC-08 | AC-08,31 | AC-08,31 | C/X/P |
| F-06 | AC-09,16 | AC-09,16 | AC-09,16 | AC-09,16 | AC-09,16 | C/X/P |
| F-07 | AC-05,06,08 | AC-05,06,08 | AC-05,06,08 | AC-05,06,08 | AC-05,06,08 | C/X/P |
| F-08 | AC-10,14,29 | AC-10,14,29 | AC-10,14,29 | AC-10,14,29,31 | AC-10,14,29,31 | C/X/P |
| F-09 | AC-10,26,29 | AC-10,26,29 | AC-10,26,29 | AC-10,26,29 | AC-10,26,29 | C/X/P |
| F-10 | AC-10,15,16,24 | AC-10,15,16,24 | AC-10,15,16,24 | AC-10,15,16,24,31 | AC-10,15,16,24,31 | C/X/P |
| F-11 | AC-20,21 | AC-20,21 | AC-20,21 | AC-20,21,31 | AC-20,21,31 | — |
| F-12 | AC-22 | AC-22 | AC-22 | AC-22,31 | AC-22,31 | — |
| F-13 | AC-25,26 | AC-25,26 | AC-25,26 | AC-25,26,31 | AC-25,26,31 | — |
| F-14 | AC-23 | AC-23 | AC-23 | AC-23,31 | AC-23,31 | — |
| F-15 | AC-23,24,34,39 | AC-23,24,34,39 | AC-23,24,34,39 | AC-23,24,31,34,39 | AC-23,24,31,34,39 | — |
| F-16 | AC-24,40 | AC-24 | AC-24 | AC-24,31 | AC-24,31 | — |
| F-17 | AC-03,18,25 | AC-03,18,25 | AC-03,18,25 | AC-03,18,25 | AC-03,18,25 | — |

Raport rozwija każdy wiersz na podfunkcje wymienione w sekcji 3 planu i zapisuje wynik
każdej podfunkcji, a nie tylko wspólny PASS wiersza. Nieobsługiwana opcja backendu
ma własne N/A-capability, wersję i dowód braku wsparcia. F-16 nie obejmuje wielu okien;
wszystkie zachowania F-16 przechodzą AC-24 na każdym kliencie. Local Quit
jest testowany na hoście; dla remote PASS oznacza brak możliwości zdalnego Quit.

| Poziom | Co może potwierdzić |
| --- | --- |
| P Test czysty | Parser, reducer, kolejność, warunki, błędy i porównanie projekcji. |
| C Test kontekstu GPUI | Własność runtime'u, subskrypcje, generacje, brak wymogu Window. |
| T Test kontraktu i integracji izolowanej | Prawdziwe kodowanie gRPC, fake proces, tymczasowa baza/repo, kontrolowane błędy i rozłączenia. |
| N Test natywny | TLS i magazyn systemowy na danej platformie, realny PTY, klient mobilny i interakcje platformowe. |
| E GUI i E2E | Pełna ścieżka użytkownika z rzeczywistymi widokami i backendem. Wymaga uprzedniej zgody zgodnie z testing.md. |
| R Przegląd | Zasadność abstrakcji, kompletność kontraktu, brak sekretów i zgodność migracji wydaniowej. |

Stosujemy [reguły testowania repozytorium](testing.md). Istniejące testy są pierwszym
wyborem. Nowy test musi chronić opisany wynik albo konkretną regresję. Nie dodajemy
testów przechowywania pól DTO, getterów, liczby traitów ani asercji kopiujących implementację.
W zwykłych testach nie używamy modeli, prawdziwych kont ani magazynu sekretów użytkownika.

## 2 Stanowisko testowe

- Dwa sparowane hosty A i B z odrębnymi katalogami danych, identyfikatorami i tymczasowymi repo.
  Na jednym komputerze A i B używają osobnych `CANOPY_DATA_DIR` (OWN-11).
- Trzeci klient C, który obserwuje oba hosty; dodatkowy klient D do wyścigów.
- Klient MCP M z osobnym grantem OAuth na A i B.
- CLI Claude i Codex w wersjach zapisanych w raporcie. Nagrania w repo są bazą regresji.
- OpenCode przez ACP jako rzeczywisty agent odniesienia. Druga definicja ACP oraz
  kontrolowany agent protokołu sprawdzają niezależność od nazwy i nietypowe opcje.
- Sterowany fake backend zapisuje efekty do niezależnego pliku lub dziennika procesu.
  Licznik potwierdza zewnętrzny skutek, a nie tylko wywołanie helpera w testowanym kodzie.
- Baza sprzed funkcjonalności zawiera historię, profile, integracje, layout i obce tabele.
  Test używa fixture lub jawnej kopii, nigdy oryginału użytkownika.
- Deterministyczne bariery umożliwiają zatrzymanie przed commitem, po commicie,
  przed dispatch, po skutku i w trakcie snapshotu. Nie polegamy na losowych sleep.

## 3 Przypadki podstawowe

| Test | Wymagania planu | Kroki i warunek PASS |
| --- | --- | --- |
| AC-01 R | SCOPE-01..06, SIMPLE-01..07 | Porównaj każdy wiersz zakresu z działającymi operacjami i macierzą urządzeń. Dla nowej warstwy wskaż wymaganie, prostszą alternatywę i rzeczywistą korzyść. PASS: brak funkcji zastąpionej makietą, brak nieuzasadnionej warstwy i brak publikacji pośredniego MVP. |
| AC-02 P/T | DATA-01..09, OWN-08 | Utwórz bazę Rust v1 i import kopii Electrona bez znacznika Rust. Policz ID i treść, zastosuj jedną końcową migrację, odtwórz rozmowy i layout. Powtórz start. Uruchom build niewydanego schematu bez CANOPY_DATA_DIR i z niezgodnym prywatnym fingerprintem. Otwórz kopię prywatnej bazy w buildzie wydanym. PASS: jeden krok; każda rozmowa ma nowy ThreadId (UUIDv7) i wpis thread_alias, a stare chat.id rozwiązuje się do tego samego wątku; brak drugiej historii; zachowane ID pozostałych zasobów, sekrety i obce tabele; rollback DDL/adopcji/znacznika; wszystkie odmowy zachowują pliki. Wydany build odrzuca prerelease jako UnsupportedSchema. Nie obiecujemy zmiany już wydanego starego binarium. |
| AC-03 C/T | OWN-01..03, OWN-06..07, LIFE-09 | Uruchom czat bez Window/ChatView. Wyślij lokalnie i przez gRPC, dołącz/odłącz widoki i przesuń kontrolowany zegar o 60 s. Powtórz ze wstrzymaną kolejką dwóch pozycji oraz unknown. Odwiedź 33 lokalne taby. PASS: jeden proces/runtime, brak aktywacji taba hosta; bez widoków dormant zachowuje kolejkę i resume sesji. Tura, decyzja, trwający dispatch, aktywna kolejka i brak resume blokują usypianie. Ukryty lokalny tab nie usypia procesu; 33. pokazuje historię/szkic i jawny limit startu z możliwością zwolnienia sesji oraz retry. Stary start jest zamknięty. |
| AC-04 T/E | OWN-04..05, OWN-08..09, LIFE-08, RPC-06 | Połącz C z A i B. Utwórz operację dla wątku na B, przełącz nawigację na wątek na A, zmień trasę B z LAN na VPN. Wyłącz B. PASS: cel operacji pozostaje `(ThreadId, executor_epoch)` z chwili utworzenia, wątek nie zmienia środowiska, A działa dalej, HostId B pozostaje ten sam, cache B ma stale i last_seen. Inny certyfikat daje odmowę. |
| AC-05 P/T/E | AGT-01..07, CHAT-07..09 | Odtwórz istniejące nagrania Claude i Codex: tekst/thinking, command/patch, tło/subagent, goal, zgody, modele, resume i fork. PASS: dotychczasowe wyniki i cleanup zachowane; karty wynikają z rodzaju semantycznego, nie z udawania nazwy narzędzia innego agenta. |
| AC-06 P/T/N/E | AGT-04..12, CHAT-06..07 | Podłącz OpenCode ACP i drugą definicję bez edycji UI/rdzenia. Fake ACP zwraca obce ID, odmawia opcji i wysyła nieznane zdarzenie; wywołuje fs i terminal. PASS: odmowa cofa wybór, Generic zachowuje sesję; read widzi lokalny bufor hosta, nie bufor C; write przy dirty na C jest odrzucony bez zmiany pliku; root i zakres są sprawdzone. Terminal agenta jest tylko do obserwacji, implementuje output/wait/kill/release i cleanup końca runu. Auth interaktywny daje needs_host_interaction. |
| AC-07 P/T/E | CHAT-01..04, CMD-01..02 | Wyślij równocześnie z C i D, przestaw kolejkę z nieaktualną rewizją, wykonaj Steer, Stop i Resume. Wyślij Send z hosta B do wątku na A przez outbox B, zgub potwierdzenie i po restarcie B ponów ten sam command_id. PASS: jedna deterministyczna kolejność przyjęć, jeden element dla klucza `(ThreadId, executor_epoch, DeviceId/principal, command_id)`, konflikt zachowuje edycję, Stop wstrzymuje kolejkę, Resume zachowuje reguły obu native backendów; brak ukrytej zamiany Steer w Send. |
| AC-08 P/T/E | CHAT-05..06, CHAT-09, LIFE-02, SEC-04..05 | C i D odpowiadają przeciwnie na tę samą zgodę. Rozłącz C, zakończ inne narzędzie, zrestartuj run i wyślij starą odpowiedź. PASS: jeden efekt, drugi wynik already_resolved, odłączenie nie rozstrzyga prośby, stary run odrzucony. Zgoda chat ma dotychczasowy czas życia, bez nowego zakresu run i zapisu plików konfiguracji użytkownika. |
| AC-09 P/T/E | CHAT-07..09, LIFE-07, DATA-04 | Wznów konkretne ID nie-UUID przez ACP z 20 zapisanymi elementami i replay load. Otwórz rozmowę przez stare chat.id po migracji. Powtórz bez lokalnej historii i z przerwanym load. Wykonaj edit/fork i restart podczas startu forka. PASS: nadal 20 elementów bez nowego epoch; stare chat.id wskazuje ten sam ThreadId przez thread_alias; import tylko po pełnym load przy braku historii; źródło/obrazy/punkt/konto zachowane; brak silent new session i niewspieranego rewind. |
| AC-10 P/T/E | CHAT-10..12, APP-12..14, TERM-09 | Wyślij obrazy, cytat, kod, skill, wzmiankę hostowego pliku i PR, uruchom !cmd i stronę HTML. Szukaj słowa w historii poza pierwszą stroną. PASS: agent dostaje właściwy kontekst hosta, obraz istnieje przed dispatch, link i schowek działają na C, znaleziony element się doładowuje, renderer nie uzyskuje RPC. |

## 4 Błędy komend trwałość i synchronizacja

| Test | Wymagania planu | Kroki i warunek PASS |
| --- | --- | --- |
| AC-11 T | CMD-01..04, CMD-08, RPC-02, BUS-02 | Zgub accepted i ponów to samo ID przez nowe połączenie, z outboxu hosta B, potem po restarcie wykonawcy. Wyślij inne body z tym ID. Wyślij komendę ze starym executor_epoch. PASS: jeden element/efekt u wykonawcy wątku, zachowany stan i idempotency_conflict dla zmiany treści; brak dispatch po błędzie zapisu; stara epoka wykonawcy odrzucona przed dispatch. |
| AC-12 T | CHAT-02, CMD-04..07, CMD-12, SYNC-08 | Zabij host przed commit, po accepted, po attempting i po efekcie przed wynikiem. Osobno zakończ Send sukcesem do kolejki dwóch pozycji, zabij host przed dispatch, a potem po wysłaniu pozycji przed dowodem przyjęcia. PASS: brak skutku przed commit; kolejka wraca wstrzymana, a niepewna pozycja ma delivery_unknown mimo succeeded Send. Resume nie wysyła następnej pozycji przed rozstrzygnięciem. Sprawdź osobno uznanie dostarczenia, nową próbę i odrzucenie: zachowana treść, właściwa kolejność i co najwyżej jeden nowy efekt po jawnym wyborze. Stara rewizja daje konflikt. Quit nie odrzuca kolejki. |
| AC-13 P/T | CMD-03, CMD-09..11, SEC-04 | Wykonaj 150 000 komend z okresowym CloseCommandEpoch. Kompaktuj payload i ponów ID przed zamknięciem; opóźnij mutację tak, aby dotarła po zamknięciu; zgub wynik Close, ponów go i zrestartuj host. Sprawdź accepted/attempting, unknown, offline tekst oraz nowy Send z tego samego urządzenia podczas Close. Wypełnij limity X i lokalne. PASS: brak blokady zwykłego użycia i drugiego efektu, stara epoka zawsze command_retired, Close nie zwiększa licznika dwa razy; pending blokuje Close, unknown pozostaje. Outbox i composer nie tracą tekstu. X nie blokuje Y ani lokalnego Send; revoke/cleanup działa przy limicie. Zakończone lokalne komendy nie tworzą wiecznych tombstones; revoked nie liczy się do 32 ważnych tożsamości. |
| AC-14 T/E | OFF-01..07, OWN-04, CHAT-11 | Offline czytaj wątek z repliki. Napisz tekst z obrazem, zmień nazwę wątku, zleć Git, Delete i Stop. Zamknij klienta, uruchom ponownie i połącz. Osobno zmień nazwę na hoście przed reconnect. Zmień konto lub usuń wątek na hoście. PASS: odczyt działa offline; szkic/outbox wraca; zmiana nazwy odtwarza się automatycznie, a niezgodna rewizja daje jawny konflikt bez nadpisania; Send czeka na jawne działanie; zmiana kontekstu jest ujawniona; nic nie trafia do innego celu. Approve/PTY nie są odtwarzane automatycznie; Git/Delete/Stop nie są odtwarzane wcale. |
| AC-15 P/T | SYNC-01..05, OWN-06 | Wstrzyknij zdarzenia przed/po granicy snapshotu i w trakcie transferu; zduplikuj i zgub deltę; podziel wielobajtowy UTF-8; opróżnij ring; zrestartuj host. PASS: brak cichej luki/duplikatu; stara epoka i błędny offset odrzucone. Resync jest przejściowy: po ustaniu faultu i nadmiarowego ruchu zgodny snapshot i następna poprawna delta są zastosowane w ≤10 s przy limitach AC-30, także dla dużego snapshotu. |
| AC-16 P/T | SYNC-05..09, CHAT-12, DATA-06 | Odczytuj strony podczas napływu tekstu i Delete. Wczytaj trimmed checkpoint, rekord za duży także po skróceniu oraz nowszą wersję. PASS: stabilna granica lub jawne unieważnienie; brak pełnego live append do skróconej treści; ograniczony placeholder zachowuje ID i ujawnia stratę. Dalszy poprawny TurnEnd zapisuje się i Quit kończy; saved nie udaje zachowania brakującej treści. |
| AC-17 T/E | CMD-04, SYNC-06..08, LIFE-05, RPC-05 | Zasymuluj read-only, disk-full i błąd finalizacji. Odczytuj stream i wykonaj Quit. PASS: live działa do jawnego błędu zasobu, brak fałszywego accepted/saved, poprzedni checkpoint pozostaje czytelny, Quit nie zataja błędu i pozwala naprawić zapis. |
| AC-18 C/T/E | LIFE-01..10, RPC-03..04 | Osobno wykonaj Detach, timeout RPC, Interrupt, Restart, Close remote view, Close host tab/Delete, Close host project i Quit host. Utwórz pracujący czat mobile bez pane'a hosta. PASS: skutek z tabeli lifecycle; guard projektu wymienia też ten czat i klientów, guard taba tylko jego pane'y; ryzyko pochodzi z runtime'u. Utrata klienta niczego nie kasuje; cleanup ma jeden wynik/retry; Quit zachowuje historię i kolejkę. |
| AC-19 T/N | RPC-01..07, SCOPE-03..04 | Na prawdziwym natywnym stosie wykonaj unary, server stream, bidi, TLS/pinning/mTLS, anulowanie i zmianę trasy. Zatrzymaj backend jednego agenta. PASS: transport działa na obu mobile OS i desktopach; Health/keepalive/stan domenowy nie są mylone; inne usługi pozostają gotowe. |

Dodatkowe warianty AC-12/13: GetCommand zwraca not_found, po czym opóźniona komenda
dociera do hosta. PASS: klient ponawia ten sam ID w otwartej epoce i uzgadnia wynik
przed Close, bez traktowania not_found jako dowodu niewykonania. Rozstrzygnij także
delivery_unknown zapisu pliku lub Push. PASS: archiwizacja zachowuje adnotację
użytkownika, nie powtarza efektu i pozwala zwolnić wpis przy kolejnym Close.

## 5 Pozostałe funkcje i bezpieczeństwo

| Test | Wymagania planu | Kroki i warunek PASS |
| --- | --- | --- |
| AC-20 T/N/E | APP-01..03, SEC-08 | Otwórz plik na C i D. Zapisz z C, potem z D; zmień plik przez agenta. Sprawdź BOM/CRLF/tryb, delete i symlink/junction po otwarciu. PASS: drugi zapis daje konflikt i zachowuje tekst, brak wyjścia poza root, brak cichego odtworzenia skasowanego pliku. Nie deklarujemy ochrony przed niewykrywalnym zewnętrznym wyścigiem check/rename. |
| AC-21 T/E | APP-04..05, LIFE-03, DATA-08 | C ma dirty buffer. D usuwa worktree, C jest online, potem offline. Zrestartuj host przed preflight D. Osobno wykonaj save/discard, revoke i Dismiss. PASS: online wymaga decyzji właściciela; offline ryzyko przetrwa restart; wpis znika tylko zgodnie z kontraktem; po Delete C zachowuje draft i wskazuje nowy cel. Dismiss i nawigacja D nie odrzucają tekstu C. |
| AC-22 T/N/E | APP-06..07, LIFE-03..05 | Na tymczasowych repo wykonaj stage/unstage/discard, commit/hook/sign, Pull/Push oraz create/merge/remove worktree. Zmień HEAD/indeks po preflight i wstrzyknij błąd cleanupu. PASS: istniejące guardy, poprawne częściowe wyniki i konflikt; brak powtórnego commita/usunięcia po niepewnym RPC; inne repo działa. |
| AC-23 T/N/E | APP-08, APP-10..11, SEC-06..07 | Wykonaj istniejące operacje GitHub/Jira/YouTrack: listy, filtry, edit, załącznik i task do worktree. Zmień i usuń credential z błędem zapisu. PASS: konto i kontekst hosta poprawne, brak tokenu na kliencie poza polem write-only, błąd zachowuje stare konto i ponawialny cleanup. Prompt OS daje needs_host_interaction. |
| AC-24 C/T/E | APP-09, APP-12, APP-15, OWN-03..05, OWN-10 | Otwórz okno główne hosta, zdalny desktop C i mobile na różnych zasobach. Zmień zoom, layout, szkic i aktywny projekt na każdym kliencie. Otwórz notes, diff i obsługiwane media. PASS: nawigacja, layout i szkic C oraz mobile są stanem ich urządzeń i nie zmieniają stanu globalnego hosta; ustawienia urządzenia są niezależne; hostowe konfiguracje wspólne, dostępne funkcje mają rzeczywiste wyniki, lokalny UI zachowuje tokeny/klawiaturę/Reduce Motion. |
| AC-25 T/N/E | TERM-01..05, TERM-08 | C ma input; D ogląda i próbuje pisać/resize. Przejmij jawnie, zgub połączenie, przekrocz lease i ponów stare bajty/token. PASS: jeden właściciel, stary token odrzucony, brak automatycznego replay input, proces trwa do Stop, cleanup potwierdzony także na Windows. |
| AC-26 T/N/E | TERM-06..09, APP-12 | Uruchom TUI z alternate screen, szerokie/łączone znaki, kolory, tryby paste i IME. Zmieniaj scroll i zaznaczenie na C i D; wymuś resync. PASS: właściwa projekcja i tryby, niezależny viewport, jawny utracony scrollback, brak zepsutych sekwencji ANSI; !cmd zachowuje limit i cleanup. |
| AC-27 T/N | SEC-01..03, SEC-09, RPC-06 | Czysty klient bez certyfikatu wykonuje enrollment. Podmień klucz po lokalnej zgodzie, równocześnie zużyj token, sprawdź wygaśnięcie i odczyt przed zgodą. Zmień fingerprint TLS oraz klucz SSH. PASS: tylko prawidłowe enrollment może nie mieć starego credentialu; związanie token/klucz/zakres i mTLS działa. Zwykłe RPC przez SSH/VPN bez auth jest odrzucone, także przy poprawnym tunelu. |
| AC-28 T/N/E | SEC-04..08, CMD-03, LIFE-06 | Zmniejsz uprawnienia lub revoke podczas streamu, PTY lease i accepted przed dispatch. Prześlij credential do pola write-only. PASS: odcięcie w ≤1 s, brak kolejnych niedozwolonych efektów i odczytu wyników; wykonane skutki nie są przedstawione jako cofnięte. Skan konfiguracji odpowiedzi/logów/inboxu nie znajduje sekretu testowego. Nie skanuj uprawnionego pliku zawierającego celowo ten sekret jako dowodu niezgodności. Remote Quit jest niedostępne. |
| AC-35 T/N | SEC-01..04, SEC-09..10 | Sparuj A z B jednym zaproszeniem; na obu mają pojawić się projekty, worktree i czaty drugiego komputera, również dodane później. Ponów zakończenie parowania po utracie odpowiedzi, zrestartuj obie aplikacje i zmień trasę LAN/VPN/SSH. PASS: brak drugiego QR, duplikatów urządzeń i utraty szkiców; połączenie zwrotne SSH używa własnego tunelu. Ręczna pauza i ograniczenia przetrwają ponowienie i restart, revoke nie jest cofane. Aktualizacja starego prywatnego defaultu następuje raz i atomowo, bez zmiany ID, credentiali i epok. |
| AC-29 T/E | CHAT-11, OFF-06, APP-13..14, PERF-01 | Przerwij upload, powtórz identyczny chunk, podaj inne bajty pod tym samym offsetem, zły hash i nadmiarowy rozmiar. Wykonaj Finish równolegle z Delete/revoke, crash po publikacji i zmianę pliku przed zastosowaniem obiektu. PASS: obiekt raz albo jawna odmowa, brak podmiany załącznika, SaveFile sprawdza aktualny warunek. Osiągnij limity niepodpiętych obiektów, wyników i lokalnych załączników; brak kopii bulk w inboxie. Fragmenty wygasają, fork zachowuje obraz; HTML/media wymagają auth i nie otrzymują RPC. |

## 6 Wydajność i zachowanie pod obciążeniem

AC-30 obejmuje SYNC-10, PERF-01..05 oraz limity z sekcji 13 planu.

Przygotuj 100 000 zachowanych elementów historii oraz 8 sesji fake backendu.
Każda sesja emituje 100 zmian na sekundę po 1 KiB. Podłącz 8 klientów i 32 subskrypcje.
Jeden klient nie odczytuje danych. Równocześnie wykonuj bulk oraz co najmniej
1 mutację control na sekundę. Rozgrzewka trwa 60 s, pomiar 10 min.

Wykonaj osobne serie: LAN z RTT ≤10 ms oraz sieć z RTT ≤100 ms.
RTT potwierdź w tym samym przebiegu. Zapisz CPU/RAM/dysk, konfigurację sieci,
rozmiary danych i liczby próbek. Sprzęt musi wystarczać do nieobciążonego baseline'u;
nie wolno ukryć wyniku FAIL przez zmianę progu po wykonaniu testu bez aktualizacji planu.

PASS wymaga jednocześnie:

- p95 od wysłania komendy do accepted: ≤100 ms LAN i ≤300 ms dla drugiej serii;
- p95 host parse do enqueue: ≤30 ms; p95 klient receive do apply: ≤50 ms;
- zachowania wszystkich limitów kolejek i ringa oraz resource_exhausted zamiast OOM;
- jawnego resync wolnego klienta i poprawności projekcji u pozostałych;
- pierwszej strony historii w limicie strony, bez pobrania całego transkryptu;
- braku czekania live streamu na 500 ms debounce zapisu;
- braku nieograniczonego wzrostu pamięci w końcowych 5 minutach testu;
- braku stałych redraw po zakończeniu aktywności i bez aktywnej animacji.
- logów pozwalających skorelować command_id i etapy bez promptów i sekretów.

Od send do accepted mierz czas monotoniczny jednego klienta. Hostowe etapy mierz
na hoście, a receive do apply na kliencie. Nie odejmuj surowych timestampów A i C.
Czas modelu nie jest narzutem synchronizacji. Test aplikacji z prawdziwym modelem
jest osobnym dowodem, a nie zamiennikiem powtarzalnego testu transportu.

## 7 Obowiązkowa kwalifikacja natywna

AC-31 obejmuje SCOPE-01 do SCOPE-06, RPC-07, APP-11 do APP-15 i LIFE-08.

| Host | Klient obowiązkowy | Trasy |
| --- | --- | --- |
| macOS Apple Silicon | Desktop macOS i Windows 11 x64 | LAN TLS, SSH tunnel, Tailscale/VPN |
| Windows 11 x64 | Desktop macOS i Windows 11 x64 | LAN TLS, SSH tunnel, Tailscale/VPN |
| macOS i Windows jako odrębne serie | Fizyczny iPhone i fizyczny Android | LAN TLS, SSH tunnel oraz Tailscale/VPN |

Test każdej kombinacji obejmuje pairing, odczyt katalogu, Send/stream, decyzję, plik,
Git, terminal, reconnect i revoke. Pełna macierz funkcji z sekcji 3 planu jest
odbierana na każdym typie klienta. Dodatkowo mobile musi przejść:

1. Zmianę Wi-Fi na sieć komórkową z aktywnym VPN.
2. Suspend, powrót do foreground i restart procesu klienta.
3. Wysyłkę outboxu po sprawdzeniu kontekstu.
4. Klawiaturę ekranową, zaznaczenie, paste i obsługę zgody bez hover.
5. Dostępność etykiet, brak uciętych akcji i zachowanie lokalnego drafta po błędzie.
6. Samodzielne utworzenie tunelu SSH przez telefon, bez tunelu na pomocniczym komputerze;
   pinning klucza SSH, cleanup i reconnect. Zerwanie tunelu nie zatrzymuje agenta.

Brak gwarantowanego push przy uśpieniu jest jawnym ograniczeniem produktu.
Foreground musi zsynchronizować stan. Test nie może zakładać ciągłego życia gRPC
w tle wbrew zasadom iOS/Android.

AC-32 jest końcowym przeglądem wydania. Sprawdza DATA-01 do DATA-09 oraz SIMPLE-01
do SIMPLE-07 po zakończeniu wszystkich etapów. PASS: końcowa migracja uruchamia się raz
na starej bazie; historia repo nie zawiera przejściowych migracji tej funkcji; nie ma
aktywnego starego remote WebRTC, pakietu `canopy.remote.v1` ani nieudokumentowanego obejścia szyny komend.
Dokumentacja modułów opisuje rzeczywiste zachowanie. Każde N/A ma uzasadnienie.

AC-33 sprawdza brak regresji Linux według sekcji 14 planu. Wykonaj istniejące
adekwatne testy lokalnego runtime'u, historii, plików i cleanupu z `--locked` oraz
lokalny smoke czatu Claude/Codex, odtworzenia historii, taba i Quit. PASS wymaga
wyniku i środowiska; smoke GUI wymaga wcześniejszej zgody. Ten przypadek nie jest
deklaracją nowego produkcyjnego portu ani zamiennikiem macierzy macOS/Windows.

AC-34 T/N obejmuje CMD-02..05, APP-10, SEC-06..07 i wersjonowanie sekretów w sekcji 8.
Ponów zapis z tym samym i innym sekretem pod tym samym command_id. Zatrzymaj host
po zapisie vault, przed/po commicie i podczas zmiany konfiguracji. Obróć credential
równolegle i uruchom cleanup. Zasymuluj niedostępny vault oraz utratę klucza MAC.
PASS: zaakceptowany zamiar nie zmienia wersji sekretu; identyczne retry nie tworzy
kopii, zmienione daje konflikt; potrzebna wersja nie jest usuwana, a osierocona
jest rozpoznana do cleanupu. Błąd zachowuje stare konto. Brak plaintextu i zwykłego
hasza słabego sekretu w bazie/diagnostyce; utrata klucza nie pozwala powtórzyć efektu.

## 8 Wątki szyna komend replikacja i MCP

| Test | Wymagania planu | Kroki i warunek PASS |
| --- | --- | --- |
| AC-36 P/T/R | BUS-01..05, RPC-08..13 | Uruchom test kontraktowy deskryptor proto, rejestr komend i lista narzędzi MCP. Usuń kolejno RPC, narzędzie i wpis rejestru dla jednej komendy. Uruchom `buf lint`; od pierwszego wydania v1 także `buf breaking` względem wydanego deskryptora. Wykonaj tę samą komendę przez UI, gRPC i MCP z brakującym uprawnieniem. Odczytaj plik projektu z Git i bez Git przez `ProjectRef`, z `worktree_id` i bez niego, oraz plik wątku przez `ThreadRef`. PASS: test wykrywa każdy brak; brak `Execute`, `ChatRef`, `WorkspaceRef` i `canopy.remote.v1`; niezgodny `worktree_id` odrzucony przed I/O, a `ThreadRef` rozwiązuje projekt i worktree wątku; ta sama odmowa i ten sam fingerprint dla trzech wejść; zmiana ustawienia zwraca nową rewizję. |
| AC-37 T/N | REPL-01..07, OWN-09, DATA-05 | Utwórz wątki na A i B. Zmień metadane wątku A z B. Wyłącz A, czytaj i przeszukuj jego transkrypt na B, potem usuń wątek na A i włącz A. Przerwij strumień katalogu w trakcie snapshotu. PASS: B ma pełną replikę tylko do odczytu w tych samych tabelach historii; zmiana z B przechodzi przez inbox A z kontrolą rewizji; usunięcie znika z B po snapshocie; brak relay i brak lokalnego zapisu repliki; wątek nie zmienia środowiska. |
| AC-38 T/E | MCP-01..05, BUS-02 | Połącz M z A i B z grantem Ask dla files_git, Deny dla terminal i admin bez potwierdzeń. Wylistuj wątki, zmień konfigurację wykonującą kod, wykonaj komendę Ask i Deny. Odwołaj principal M na A. PASS: M widzi wątki obu hostów z HostId i metadanymi; Ask czeka na potwierdzenie, Deny jest odrzucone, admin działa bez potwierdzenia; każda komenda UI ma narzędzie; revoke na A nie zmienia M na B ani innych principali. |
| AC-39 T | APP-09, REPL-06 | Zmień preferencję użytkownika na A i B offline, połącz hosty. Zmień ustawienie hosta i urządzenia. PASS: preferencja użytkownika zbiega się według HLC/LWW na obu hostach; ustawienie hosta zostaje na hoście; ustawienie urządzenia nie przestawia innego urządzenia. |
| AC-40 C/E | OWN-10..11 | Uruchom drugą instancję aplikacji tego samego użytkownika OS na tym samym katalogu danych. Wykonaj akcję widoku z MCP. Osobno uruchom instancję z innym `CANOPY_DATA_DIR`. PASS: druga instancja nie pokazuje osobnego stanu; istnieje jedno okno główne; akcja MCP zmienia stan globalny renderowany przez to okno bez adresowania okna; instancja z izolowanym katalogiem startuje niezależnie. |

## 9 Wymagane artefakty odbioru

Uzupełnienie AC-36 dla D16: lokalny terminal Private/scratch bez projektu i wątku
przechodzi przez bus z EnvironmentRef. Test granicy ma przyjąć lokalne open/close
i odrzucić te same intencje dla uwierzytelnionego urządzenia, również z grantem
Private i pełnymi scope, przed dedupe lub ujawnieniem wyniku. Publiczny Terminal
nie przyjmuje EnvironmentRef ani dowolnej ścieżki. Katalog, historia i strumień
nie ujawniają terminala Private/scratch także przy cwd równym projektowi oraz
korzeniu `*`; usunięcie projektu przerywa dalsze ujawnianie historii i wyjścia.
CI sprawdza usunięte referencje,
`buf lint`, aktualność generatora mobile oraz testy biblioteki i binarium.
Do pierwszego wydania brak baseline breaking nie blokuje niewydanej migracji;
od wydania CI wymaga niezmiennego tagu `canopy-api-v1`. Rzeczywisty rejestr MCP
pozostaje jawnym TODO następnego zadania, więc jego parytet nadal jest NOT RUN.

Raport końcowy musi zawierać:

- Rewizję kodu i sumę kontrolną jedynej migracji wydaniowej.
- Wersję .proto, CLI/backendów, bibliotek transportu i platform.
- Wyniki AC-01 do AC-40 wraz z poziomem dowodu i identyfikatorami komórek macierzy funkcji i platform.
- Wyniki istniejących adekwatnych testów oraz nowych regresji z uzasadnieniem.
- Pomiary AC-30 i logi failpointów bez danych użytkownika i sekretów.
- Wynik przeglądu złożoności i listę świadomych ograniczeń platform/protokołów.

Kontrole kodu dobiera się według zmiany. Cargo zawsze używa `--locked`.
Typowe punkty startowe to `cargo test --locked --lib chat::`, istniejące
`chat_process`, `chat_claude_fixtures`, `chat_codex`, `chat_queue`, `chat_permissions`,
`chat_history`, `chat_goal`, `files`, `git_*`, `terminal*`, `settings` i testy integracji.
Są to nazwy istniejącej ochrony, a nie polecenie uruchomienia całej suite'y bez oceny.
Format i lint: `cargo fmt --all -- --check` oraz właściwy zakres
`cargo clippy --locked --all-targets -- -D warnings`.

Pomyślne testy parsera, build i Clippy nie potwierdzają GUI, mobile, TLS na urządzeniu,
rzeczywistego CLI ani opóźnień. Oczekiwanie na zgodę E2E pozostawia przypadek NOT RUN.
