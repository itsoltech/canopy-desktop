# Ustawienia i zgodność SQLite

Pierwszy etap stanu aplikacji: kontrakt preferencji, repozytorium SQLite i import
bazy Electrona. Kontrolki General są podłączone przez wspólny SettingsState; opis integracji
i ograniczeń znajduje się w [app-state.md](app-state.md).

## Kontrakt

Tabela Electrona: `preferences(key TEXT PRIMARY KEY, value TEXT NOT NULL)`.
Publiczne API zapisuje wyłącznie poniższe klucze:

| Klucz | Typ w Rust / zapis | Wartość domyślna |
| --- | --- | --- |
| reopenLastWorkspace | bool / true, false | true |
| notch.enabled | bool / true, false | false |
| perf.hud.enabled | bool / true, false | false |
| newTab.toolId | niepusty identyfikator narzędzia | shell |
| newWorktree.toolId | niepusty identyfikator narzędzia | shell |

Brak wartości oznacza fallback w pamięci, bez dopisywania do bazy.
Niepoprawna wartość daje ostrzeżenie zawierające tylko klucz i fallback;
odczyt nie naprawia danych automatycznie. Nieznane identyfikatory narzędzi
zachowujemy — sprawdzanie dostępności narzędzi będzie osobną operacją.
Reset usuwa wybrany klucz.

Nieznane preferencje, inne tabele i zaszyfrowane wartości pozostają zachowane.
API nie udostępnia sekretów ani ich deszyfrowania. Zgodność formatu SQLite
nie zastępuje integracji z Electron safeStorage.

## Schemat i import

Obsługujemy ciąg migracji Electrona 1–11 i sprawdzamy strukturę preferences.
Nie wykonujemy migracji Electrona. Nowszy lub nieciągły schemat jest odrzucany,
również gdy zmieni się podczas pracy workera. Nowa pusta baza Rust ma własny
marker `_canopy_rust_meta` w wersji 1; nie udaje kompletnej bazy Electrona.

Import otwiera źródło tylko do odczytu i korzysta z SQLite Online Backup,
obejmującego zatwierdzone dane w WAL. Sprawdza integralność oraz publikuje
samodzielny plik atomowo, bez nadpisywania istniejącego celu. Nowe katalogi
mają uprawnienia 0700 na Unix; katalog danych Windows dostaje chroniony DACL
dla LocalSystem i właściciela. Limit czasu kopii: 30 sekund.

Lokalne źródło produkcyjne:
`~/Library/Application Support/canopy/canopy.db` (migracje 1–11).
Baza `canopy-dev` ma migracje do 18 i jest obecnie nieobsługiwana.
Kopia do dalszej integracji na macOS:
`~/Library/Application Support/Canopy Rust/canopy.db`.
Aplikacja desktopowa otwiera ją automatycznie przez SettingsState. Na Windows
baza znajduje się w `FOLDERID_LocalAppData/Canopy Rust/canopy.db`. Jawny
`CANOPY_DATA_DIR` zastępuje katalog platformowy, głównie dla izolowanych testów.
Wybór katalogu nie zmienia automatycznie ścieżek cwd ani danych workspace'u i
nigdy nie uruchamia fallbacku do bazy Electrona.

## API i CLI

`SettingsClient` posiada osobny wątek SQLite oraz asynchroniczne odpowiedzi.
Render i wątek UI nie wykonują SQL. Kolejka mieści 64 komendy; przepełnienie
zwraca błąd. Pakiet do 64 zmian jest jedną transakcją; wynik zawiera zatwierdzony
snapshot. SQLite busy timeout wynosi 2 sekundy. `shutdown` kończy wcześniej
zakolejkowane operacje i zamyka połączenie przed potwierdzeniem.

```sh
cargo build --locked --release --bin canopy-settings
target/release/canopy-settings import \
  "$HOME/Library/Application Support/canopy/canopy.db" \
  "$HOME/Library/Application Support/Canopy Rust/canopy.db"
target/release/canopy-settings show \
  "$HOME/Library/Application Support/Canopy Rust/canopy.db"
target/release/canopy-settings set /path/to/working.db notch.enabled true
target/release/canopy-settings init /path/to/new.db
```

Polecenie show otwiera bazę tylko do odczytu i wypisuje jedynie kontrakt pięciu
preferencji. Set służy do bazy roboczej; nie kierujemy go do oryginału Electrona.
Szczegóły błędów SQLite nie są wypisywane, aby nie ujawnić treści danych.

## Sprawdzenia i kolejny etap

Testy integracyjne obejmują trwałość po restarcie workera, aktywny WAL,
zachowanie obcych danych, rollback całego pakietu, klientów współbieżnych,
walidację wartości, odrzucenie nieobsługiwanych schematów i zmianę wersji
przez inny proces. Fixture zawiera schemat produkcyjny bez danych użytkownika.

Wspólny stan, powiadomienia między oknami i kontrolki General są podłączone.
Profile i sekrety przechowują w SQLite tylko konfigurację i nieprzezroczyste
referencje; wartości sekretów należą do Keychain lub Windows Credential Manager.


## Stan projektów Rust

SettingsClient obsługuje także load_projects/save_projects na tym samym workerze.
Własna tabela _canopy_rust_projects przechowuje wersjonowaną listę otwartych
folderów i aktywną ścieżkę. Nie zastępuje tabel projektów Electrona.
Kontrakt, odtwarzanie i obsługa błędów: [app-state.md](app-state.md).

Pełną sesję obsługują load_session/save_session. Session i indeks projektów
są zapisywane atomowo; po utworzeniu sesji save_projects jest blokowane.
Szczegóły: [persistence.md](persistence.md).
