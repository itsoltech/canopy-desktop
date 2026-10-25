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

Wspólny przebieg na Linux (2026-10-25), z `--locked`:

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
kwalifikacji natywnej. Clippy nie zgłasza błędów kompilacji; istniejące ostrzeżenia
platformowego kodu podglądów są oddzielne od wyniku testów.

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
