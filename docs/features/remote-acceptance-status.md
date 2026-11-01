# Agent runtime i native remote — przekazanie do odbioru

Ten dokument opisuje stan implementacji i granice dowodów. Wymagania pozostają
w [planie domenowym](../agent-runtime-remote-plan.md) i
[macierzy odbioru](../agent-runtime-remote-acceptance.md). Test komponentu nie
zalicza automatycznie scenariusza GUI ani komórki macierzy platform.

## Zaimplementowane zachowanie

- Runtime czatu należy do aplikacji. Native Claude/Codex i ACP korzystają ze
  wspólnej fasady. Zdalny widok nie uruchamia lokalnego taba hosta; odłączenie
  klienta pozostawia pracę na hoście. Quit hosta kończy pracę.
- SQLite przechowuje katalog, inbox, trwałą kolejkę, referencje artefaktów,
  uprawnienia i dirty presence. Retry zachowuje pierwotne ID i treść komendy.
  Niejednoznaczny efekt pozostaje `delivery_unknown` do jawnej decyzji.
- Natywne gRPC obejmuje katalog, cały czat, pliki, Git/worktree, terminal,
  Tasks, ustawienia, urządzenia i transfer artefaktów. Handlery korzystają
  z właścicieli rzeczywistego stanu aplikacji.
- Desktop i mobile mają niezależne połączenia z hostami, pairing, przypięcie TLS,
  własne tunele SSH, reconnect, historię, decyzje, edycję, shell, pliki/Git,
  Tasks, ustawienia, media i terminal. Offline outbox przyjmuje tekst/obrazy;
  kontekst pliku/PR wymaga jawnego wysłania online.
- Snapshot katalogu jest strumieniowany i instalowany po końcowej granicy.
  Wolny odbiorca nie blokuje pracy agenta; luka wymaga jawnego resync.
  Przyjęcie wiadomości usuwa tylko wysłaną rewizję szkicu, także przy zmianie
  samych obrazów lub kontekstu.
- Procesowa diagnostyka mierzy etapy osobno i nie zawiera promptów, ścieżek
  ani sekretów. Preferences → Connections → Advanced settings → Copy diagnostics eksportuje
  lokalny snapshot. `HostService.ReadDiagnostics` wymaga Admin/Private.

## Dowody i pozostały odbiór

Wszystkie usługi, codegen i klienci używają API v1; poprzedni pakiet i adaptery
zgodności są usunięte. Adapter Files udostępnia listowanie, search,
read, save, create i dirty presence pod typowanym celem projektu albo wątku.
Sprawdzenia celu nie wykonują I/O zasobu; retry korzysta z zapisanej autoryzacji
korzenia przed odczytem receipt lub konfliktem fingerprintu. Bieżąca dokumentacja
zakresu i ograniczeń jest w [API v1](../api-v1.md).
Adapter Git wybranego checkoutu używa ProjectRef i istniejącego właściciela
preflightów, blokad, libgit2 i lifecycle. Inbox zachowuje również autorytatywne
korzenie wszystkich celów, atomowo z przyjęciem i osobno od fingerprintu klienta.
Terminal używa ProjectRef dla pięciu mutacji oraz zachowuje dotychczasowy protokół
streamu i historii. Retry lease nie przejmuje wejścia od późniejszego holdera.
Artifacts ma osobne komendy Begin/Finish/Cancel dla obrazu, pliku i zadania z
zachowaniem ich zakresów. Finish po zamknięciu projektu korzysta z autorytatywnego
wyniku Begin, a własny retry z zapisanego zakresu. Settings używa EnvironmentRef
i domenowych edycji z niejawnymi wersjami sekretów. Dokładne RPC klienta pozostaje
w dotychczasowym vaulcie; zwykły journal nie przechowuje sekretów.
Host i Enrollment używają v1 z EnvironmentRef. Revoke jest komendą busa z efektem
i receipt w jednej transakcji. Close/Resolve zachowują osobny wąski protokół;
odczyt pełnej tożsamości ponownie autoryzuje oryginalną operację. Testy obejmują
rollback revoke przy pełnym inboxie i lokalny revoke/cleanup przy limicie.
Test wyścigu Close z admission potwierdza wspólną granicę workera. Testy TLS
obejmują ponowny Close, zachowanie Unknown/Resolve i odebranie uprawnień.
Peer używa dedykowanej komendy ADMIN/Private; testy obejmują dokładny retry
po utracie potwierdzenia, konflikt intencji i odebranie ADMIN.
Testy retry obejmują przyjęty, niewykonany Files oraz Begin/Finish, utratę planu
Git i usunięcie celu. Restart rzeczywistej bazy odrzuca dawną komendę terminala
i zachowuje Unknown bez ponownego uruchomienia. Odebranie dostępu podczas
skutku zatrzymuje ujawnienie odpowiedzi, zachowując znany wynik w inboxie.

| Wymaganie | Stan implementacji migrowanych zasobów | Pozostała granica |
| --- | --- | --- |
| CMD-02/03/12 | Wspólny fingerprint, dedupe, ponowna autoryzacja i dokładne RPC w journalu klienta | Niezależny review i odbiór natywny |
| RPC-09/13 | ProjectRef z metadanych katalogu albo cel wątku, bez równoległego projektu | Lokalny wyjątek terminala Private opisany w D16; bez zdalnego rozszerzenia |
| RPC-10/11 | Typowane komendy wszystkich usług, w tym Tasks, z receipt i metadanymi porównywanymi z rejestrem | Rzeczywisty rejestr MCP jest następnym zadaniem |
| CMD-08/10/11, Host | Revoke atomowe z receipt; osobny protokół Close/Resolve, zachowanie unknown i pełnej tożsamości; test wyścigu Close/admission | Końcowy przegląd |
| RPC-08, BUS-01/05, AC-36 | Usunięto stary proto/codegen/runtime i adaptery projekcji; lokalny edytor/create/Git/Terminal/Tasks/configuration używa busa; CI pilnuje usuniętych referencji, lint i generatora | AC-36 nie jest w całości zaliczone: MCP i odbiór GUI/natywny pozostają niewykonane |

Powyższe stany nie są wynikiem odbioru GUI, mobile ani E0.

Końcowa weryfikacja niezacommitowanej implementacji PR3 na Linux (2026-11-01):

- `cargo test --locked --lib`: 375 PASS, 1 ignored po poprawkach review.
  `cargo test --locked --bin canopy-desktop`: 168 PASS, 2 ignored.
  Oba pełne przebiegi bez błędów; znany niestabilny test cleanupu discovery
  przeszedł bez powtarzania. Wcześniej poprawiono asercję kolejności
  asynchronicznych powiadomień w teście binarium.
- `cargo clippy --locked --all-targets -- -D warnings`: PASS, bez ostrzeżeń.
- `buf lint` 1.57.2 i `node scripts/generate-remote.mjs --check`: PASS.
  Breaking względem pierwszego wydania jeszcze nie dotyczy niewydanego API.
- Mobile: `npm run typecheck`: PASS; `node scripts/test-remote.mjs`: 29 PASS.

Testy obejmują lokalny terminal bez projektu, odmowę katalogowania i odczytu
Private/scratch mimo pełnego grantu urządzenia, odcięcie historii po usunięciu
projektu oraz restart niewykonanego lifecycle katalogu bez blokującego `Accepted`.
Regresje po review obejmują trwałe odmowy uploadu i jawne rozstrzyganie bez
replay niepewnego Finish oraz restart `tasks.worktree` z usuniętym celem:
trwała odmowa nie blokuje zamknięcia epoki.
Naprawa Recovery zachowuje ThreadId, historię i wybrany root przy identycznym retry;
lokalne Delete działa również przed pierwszym otwarciem starego zapisanego czatu.
Schemat i lockfile nie zmieniły się w PR3. GUI, E0 i natywne moduły mobile
nie były uruchamiane; rzeczywisty parytet MCP nadal wymaga następnego zadania.

Historyczny wspólny przebieg na Linux (2026-10-25), z `--locked`:

- `cargo test --locked --lib`: 278 PASS, 1 ignored.
- Binarium: 117 PASS, 2 ignored; dodatkowo 19 wybranych zestawów integracyjnych:
  236 PASS, 3 ignored. Obejmują runtime store/presence, artefakty, ACP/native
  fixtures, kolejkę, historię, uprawnienia, cele, pliki/Git oraz integracje
  Jira/YouTrack i filtry/mutacje Tasks. Brak błędów w tym przebiegu.
- `npm run typecheck`: PASS. `node scripts/test-remote.mjs`: 29 PASS.
- Aktualność wygenerowanego kontraktu mobile: PASS. Porównanie Cargo.lock:
  żadna pierwotna para wersja/źródło nie zniknęła; dodano 34 wpisy zależności.
- Po tym przebiegu końcowa poprawka przyjęcia pamięci katalogu przeszła 7 testów
  celowanych. Rezerwacja poprzedza budowę kopii dla urządzenia; strony nie
  kopiują ponownie całej listy checkoutów.

Ignored pozostają niewykonanymi przypadkami, nie wynikiem PASS. Liczby dotyczą
tych konkretnych przebiegów; testy celowane po dalszej poprawce nie zastępują
kwalifikacji natywnej. Ostrzeżenia platformowego kodu podglądów z historycznego
przebiegu nie dotyczą wyniku końcowego Clippy PR3 podanego powyżej.

| Obszar | Dostępny dowód | Granica |
| --- | --- | --- |
| Własność, trwałość, kolejka, migracja | Headless GPUI, rzeczywiste tymczasowe SQLite, failpointy, restart i retry | Bez GUI na macOS/Windows |
| Native i ACP | Nagrane fixtures i kontrolowane procesy testowe, zgody i capabilities | Bez live Claude/Codex/ACP |
| TLS, Health, auth i revoke | Rzeczywiste loopback TLS/HTTP2/gRPC z wygenerowanymi certyfikatami i tymczasową bazą | Bez systemowego vault i połączenia między urządzeniami |
| Pliki, Git, Tasks, ustawienia | Tymczasowe pliki/repozytoria, rzeczywisty libgit2/SQLite, kontrolowany HTTP i wstrzyknięty vault | Bez kont providerów i natywnego Keychain/Secret Service |
| Klient desktop | Kompilacja i regresje stanu, journala, projektowania strumienia i szkiców | Bez interakcji w oknie |
| Klient mobile | TypeScript, zgodność wygenerowanego kontraktu, regresje kodeka/persistencji/strumieni | Moduły iOS/Android nie zostały zbudowane ani uruchomione tutaj |
| Obciążenie | Komponent: 8 urządzeń, 32 subskrypcje, 8 strumieni, wolny odbiorca, równoległe rezerwacje bulk, 100 tys. elementów historii | 3 s ruchu; nie jest to pełny 10-minutowy AC-30 ani pomiar LAN/render/RSS |

E0 i kwalifikacja natywna E5/E6 pozostają **NOT RUN**. AC-30/31 i GUI/E2E nie
mają PASS. Użytkownik zadeklarował test na macOS po implementacji. Testy
Windows/iOS/Android nadal wymagają właściwych stanowisk. Nie należy publikować
tej funkcjonalności na podstawie samej kompilacji i testów komponentowych.

## Uruchomienie odbioru na macOS

1. Zapewnij `protoc` w PATH (sprawdź `protoc --version`; `PROTOC` może wskazywać
   istniejący executable), Rust 1.95 i narzędzia Xcode. Zbuduj ten checkout
   poleceniem `cargo build --locked`. Nie włączaj
   `published-runtime-schema` do prywatnego testu. Wymagania środowiska builda
   pozostają w [rust-rewrite.md](../rust-rewrite.md).
2. Wskaż oddzielny katalog, na przykład:

   ```sh
   CANOPY_DATA_DIR="$HOME/CanopyRemoteAcceptance" ./target/debug/canopy-desktop
   ```

   Niewydany build odmawia użycia domyślnego katalogu danych. Do próby adopcji
   użyj odrębnej, spójnej kopii dotychczasowych danych wykonanej po zamknięciu
   aplikacji. Nie kopiuj prywatnej bazy wcześniejszego etapu na istniejący katalog.
   Niezgodny prerelease daje `UnsupportedSchema` i pozostawia plik bez zmian.
3. W Preferences → Connections włącz Remote access. Wybierz **Test on this
   computer…**. W otwartym oknie klienta naciśnij **Request access**, wróć do
   Preferences i zatwierdź **Chat** oraz wybrany projekt lub Private chats.
   Klient po zatwierdzeniu połączy się sam. Sprawdź katalog, utwórz czat z
   jawnie wybranym kontem i wyślij wiadomość. Disconnect ma odpiąć klienta,
   pozostawiając pracę na hoście.
4. Dla drugiego urządzenia wybierz **Local network / VPN → Add device…**.
   Wskaż adres osiągalny ze wspólnego Wi-Fi/VPN; skopiuj zaproszenie do
   **Connections → Add computer…** albo zeskanuj QR w mobile. Zatwierdź
   urządzenie na hoście. W **Other computers → Routes…** dodaj nazwane
   adresy Dom, Biuro, Tailscale lub SSH, ustaw kolejność i sprawdź oznaczenie
   In use. W domu trasa biurowa bez konkretnego wpisu routingu ma być
   pominięta bez połączenia TCP; po włączeniu właściwego VPN ma być dostępna.
   Każda trasa musi sprawdzać ten sam przypięty certyfikat. Osobno zweryfikuj
   Read bez Admin, potem Chat/Approve/Files/Git/Terminal/Tasks/Admin.
5. Wykonaj macierz AC-01..34 według dokumentu odbiorowego. Najpierw sprawdź
   send/stream/zgodę, rozłączenie podczas tury, retry tego samego ID, Stop,
   reconnect i Quit hosta. Sprawdź też szkic B wpisywany w trakcie wysyłania A,
   konflikt pliku, zawężenie uprawnień aktywnego klienta, terminal pod obciążeniem,
   Edit/restore, fork z obrazem i zamknięcie projektu z czatem bez lokalnego taba.
6. Zapisz osobno wersję systemu, build, trasę, RTT, backend, wynik i dowód każdej
   komórki. Diagnostykę hosta i klienta traktuj jako osobne serie zegarów.
   `render` mierzy budowę widoku lub React commit, a nie fizyczną prezentację.

Jedna migracja tej niewydanej funkcjonalności nadal znajduje się w `runtime.sql`.
Etapy wdrożenia nie tworzą łańcucha migracji prywatnych schematów. Bazową implementację zapisano i wypchnięto w `fcafcf8`; późniejsze zmiany
ustawień Connections i tras wymagają osobnego odbioru.

## Zmiany Connections i tras (2026-10-25)

Po bazowym wdrożeniu dodano prostsze ustawienia/parowanie, wspólną listę połączeń,
osiem nazwanych tras z kolejnością oraz sprawdzanie routingu przed TCP/SSH.
Dotychczasowy endpoint staje się pierwszą trasą; szkice i identity zostają.

Weryfikacja tej zmiany: 77 testów modułów remote PASS, 119 testów binarium PASS
w serii z `--test-threads=1` (2 ignored). Obejmuje kliknięcia prawdziwego widoku
w headless: wybór SSH/direct i dodanie trasy do tego samego rejestru bez parowania.
Pierwsza seria równoległa miała jeden błąd istniejącego testu cleanupu projekcji:
zdarzenie Observers dotarło po Unsubscribe, podczas gdy asercja wymaga końcowego
Unsubscribe. Ten przypadek przeszedł osobno i w serii kolejnej; kod projekcji
nie był zmieniany. Clippy all-targets kończy się kodem 0 z 11 istniejącymi
ostrzeżeniami poza zmienionymi modułami. Formatowanie i kontrola diff PASS.

Testy routingu obejmują Dom/Biuro, konkretną trasę VPN i odczyt lokalnej trasy
z kernela Linuksa. Systemowe ścieżki macOS i Windows oraz rzeczywiste przejście
Wi-Fi/VPN i wygląd ustawień na macOS nadal wymagają natywnego odbioru. Mobile
nie zmieniano w tej poprawce desktopu. Nie wykonano GUI/E2E ani dostępu do
prawdziwego vault/providerów.
