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
baza znajduje się w `FOLDERID_LocalAppData/Canopy Rust/canopy.db`. Na Linuxie
korzystamy z `$XDG_DATA_HOME/canopy-rust/canopy.db`, domyślnie
`~/.local/share/canopy-rust/canopy.db`. Zgodnie z
[XDG Base Directory Specification](https://specifications.freedesktop.org/basedir/latest/)
brak zmiennej, pusta wartość lub ścieżka względna powodują użycie domyślnego
katalogu pod `HOME`; bez poprawnego katalogu bazowego aplikacja zgłasza błąd.
Katalog aplikacji zachowuje uprawnienia `0700` na Unix. Jawny
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

### Pozycje jeszcze niedziałające

Settings pokazuje wszystkie pozycje z Electrona, ale te bez konsumenta w aplikacji
są wyszarzone (`t::faint()`), mają tooltip „Not available yet” i wyłączone kontrolki;
niczego nie usuwamy. Pozycje nawigacji z listy `UNAVAILABLE`
(`src/ui/preferences/navigation.rs`) dalej da się kliknąć — otwierają stronę z
tytułem i podtytułem „Not available yet”. Wiersz w działającej stronie wyszarza
`preferences::unavailable(id, row)` i dopisuje „Not available yet.” do opisu.

| Pozycja | Powód |
| --- | --- |
| Nawigacja: Updates, Privacy, Shortcuts, Notch, Misc, Appearance, Sidebar, Skills, Git, File Watcher | brak strony; wcześniej kliknięcie nic nie robiło |
| BACKUP (eksport/import) w stopce nawigacji | same ikony bez akcji |
| Search settings… w nagłówku | wartość pola nie jest nigdzie czytana; ⌘K tylko ustawia focus |
| General → Run setup wizard | przycisk bez akcji; w Rust nie ma kreatora |
| General → New worktree (`newWorktree.toolId`) | zapisywane, ale agent worktree wybiera dialog (`ui/worktree_dialog.rs`) |
| General → Show CPU and RAM usage (`perf.hud.enabled`) | zapisywane, ale pasek stanu nie ma próbnika CPU/RAM |

Po podłączeniu funkcji usuń jej pozycję z `UNAVAILABLE` albo opakowanie
`unavailable(...)` i z tej tabeli.

Profile i sekrety przechowują w SQLite tylko konfigurację i nieprzezroczyste
referencje; wartości sekretów należą do Keychain, Windows Credential Manager lub
Secret Service na Linuxie (`oo7`, już rozwiązywany przez GPUI). Dostęp odbywa się
poza wątkiem UI. Brak usługi lub zablokowany magazyn daje jawny błąd i zachowuje
poprzednią konfigurację. Kompilacja na Linuxie nie potwierdza działania usługi
sekretów użytkownika; prawdziwy magazyn nie był uruchamiany w testach.


## Stan projektów Rust

SettingsClient obsługuje także load_projects/save_projects na tym samym workerze.
Własna tabela _canopy_rust_projects przechowuje wersjonowaną listę otwartych
folderów i aktywną ścieżkę. Nie zastępuje tabel projektów Electrona.
Picker modelu czatu trzyma listy modeli kont i ulubione w jednym wierszu
_canopy_rust_models (save_model_catalog/load_model_catalog, [chat.md](chat.md)).
Kontrakt, odtwarzanie i obsługa błędów: [app-state.md](app-state.md).

Pełną sesję obsługują load_session/save_session. Session i indeks projektów
są zapisywane atomowo; po utworzeniu sesji save_projects jest blokowane.
Szczegóły: [persistence.md](persistence.md).

## Magazyn hosta i trwała kolejka

`initialize_runtime` na workerze SQLite przeprowadza jedną niewydaną migrację
`runtime.sql`, obejmując DDL, adopcję i fingerprint jedną transakcją. Blokada pliku
ogranicza własność katalogu danych do jednego hosta. Niezgodny fingerprint jest
odrzucany bez naprawiania danych. Sprzeczna adopcja nie wybiera dowolnego konta:
wycofuje transakcję. Historia bez metadanych pane'a otrzymuje aktywny rekord
`unresolved` z `recovery_reason`; profil i sesja ostatniej czytelnej epoki pozostają
zachowane, ale wykonanie wymaga odzyskania kontekstu. GC chroni aktywny katalog
niezależnie od listy otwartych pane'ów. Zamknięcie, ponowne otwarcie i usunięcie
historii aktualizują także katalog; jawnie zamknięte rozmowy zachowują retencję.

`accept_runtime_message` zapisuje receipt i pozycję kolejki razem. Edycja,
przesunięcie, usunięcie, pause/resume oraz trzy rozstrzygnięcia unknown korzystają
z `QueueMutation::command` i `mutate_runtime_queue`. Wymagają rewizji; retry
sprawdza znormalizowane argumenty, a receipt przechowuje wynikową rewizję.
Resend unknown archiwizuje starą pozycję i tworzy nową pod nowym ID w transakcji
jawnej komendy rozstrzygnięcia. Stare completion nie potwierdza nowej próby.
Restart zmienia attempting na delivery_unknown, wstrzymuje kolejki i zwiększa
rewizje. Sam odczyt albo otwarcie widoku nie wznawia dispatch.

Inbox utrwala wymagany scope i canonical root wyznaczony przez handler.
Dedupe, GetCommand i dispatch ponownie sprawdzają zapisany zakres. Root czatu
pochodzi z katalogu (workspace albo `private`); pozostałe handlery ustawiają
`CommandInput::with_root`. Urządzenie bez granted roots nie ma dostępu; `*`
jest jawnym grantem wszystkich roots. Budżet bajtów obejmuje także aktualizację
wyniku, a limit jednego urządzenia nie blokuje pozostałych.

`resolve_runtime_command` archiwizuje niepewność bez deklarowania potwierdzenia
zewnętrznego skutku. `acknowledge_runtime_command` usuwa tylko końcowy lokalny
wynik odebrany przez właściciela (unknown wymaga rozstrzygnięcia). Remote GC
odbywa się przy `close_runtime_command_epoch`: nierozstrzygnięte unknown zostaje,
a stara epoka nie pozwala ponowić skutku po usunięciu dedupe.

## Prywatny magazyn uploadów i artefaktów

`ArtifactStore` jest pojedynczym właścicielem plików bulk hosta. Jego blokujące
operacje należy wywoływać na executorze tła; nie wykonują I/O na workerze SQLite.
Blokada magazynu serializuje najwyżej cztery uploady, a blokada pliku wyklucza
równoczesne otwarcie drugiego magazynu w tym samym katalogu. Nie powstaje drugi
writer SQLite: nazwane operacje `SettingsClient` zapisują metadane krótkimi
transakcjami istniejącego workera. To kontroluje konkretną awarię między zapisem
bajtów i potwierdzeniem zakresu bez nowego frameworka operacji.

Begin wiąże urządzenie, cel, purpose, rozmiar, SHA-256 i media type. Klient nie
wybiera ścieżki magazynu; nazwa pliku jest wygenerowanym UUID. Katalog i pliki na
Unix mają odpowiednio 0700/0600. Limit obrazu to 64 MiB; najwyżej cztery otwarte
uploady rezerwują łącznie 256 MiB. Fragment ma najwyżej 64 KiB. Identyczne nakładanie
jest idempotentne, a zmienione bajty w potwierdzonym zakresie dają conflict.
Po `sync_all` pliku worker zapisuje scalone zakresy; crash przed tym zapisem
wymaga powtórzenia niepotwierdzonego fragmentu. Status nie odgaduje postępu z samej
długości pliku. Metadane ograniczają liczbę rozłącznych zakresów do 4096.

Finish sprawdza długość i strumieniowo liczy SHA-256 poza transakcją. Następnie
w jednej transakcji sprawdza właściciela, bieżące granty, istnienie rozmowy,
kompletność i budżet niepodpiętych obiektów (256 MiB, najwyżej 4096 obiektów).
Ta sama serializacja SQLite obejmuje Delete i revoke. Publikacja zachowuje
nazwę pliku, a ArtifactId jest UploadId; restart po commit zwraca ten sam obiekt.
Plik opublikowany nie przyjmuje kolejnych fragmentów. Cel `File` przechowuje
canonical root i względną ścieżkę, ale **nie zatwierdza warunku bazowego SaveFile**:
handler zastosowania musi sprawdzić aktualny plik i warunek w momencie zapisu.

Akceptacja Send woła `retain_for_chat` wewnątrz transakcji receipt/kolejki.
Inbox zawiera wyłącznie ID, nigdy kopię bytes. Referencja rozmowy chroni obraz
przed TTL do cleanupu jej historii. Integracja forka i innych komend bulk musi tworzyć własne referencje w ich
transakcjach przyjęcia; te handlery nie należą do API magazynu.
Usunięcie receipt zwalnia referencję komendy, a Delete usuwa wyłącznie referencje
własnej rozmowy. Download sprawdza uprawnienia przy każdym żądaniu; obserwator
z read scope może czytać obraz podpięty do dostępnej rozmowy. Samo ID nie daje
prawa dostępu. Upload i Finish wymagają chat/write scope.

`cleanup` usuwa wygasłe metadane przed usunięciem osieroconych plików; przerwany
cleanup można bezpiecznie powtórzyć. Niedokończone uploady oraz niepodpięte
obiekty mają TTL godziny bez aktywności. Otworzenie magazynu, Begin i Cancel
wywołują cleanup. Właściciel hosta musi także wywoływać go okresowo w tle i po
Delete/revoke. Trigger Delete/revoke natychmiast blokuje dalszą publikację;
retencja przyjętej wiadomości pozostaje niezależna od revocation jej nadawcy.

`read_retained_image(chat, id)` służy dispatchowi hosta: wymaga istniejącej
referencji rozmowy, purpose Image i limitu 64 MiB, następnie sprawdza długość
i SHA-256 prywatnego pliku. Nie wymaga bieżących uprawnień pierwotnego nadawcy;
przyjęta kolejka jest stanem hosta. Dispatch nadal normalizuje obraz istniejącym
`prepare_image` przed przekazaniem do modelu.

### Autoryzacja urządzeń, presence i jawne odzyskiwanie

`SettingsClient` wykonuje te operacje na tym samym workerze SQLite co kolejkę.
`lookup_runtime_device` dopasowuje dokładne DER certyfikatu leaf otrzymanego przez
transport po weryfikacji mTLS wystawcy. Nazwa ani ID z żądania nie uwierzytelniają.
Unikalność publicznego certyfikatu obejmuje również odwołane urządzenia.
`authorize_runtime_read` sprawdza aktualne revoke, zakres i root; zwraca `DeviceGrant`
z epoką komend i rewizją autoryzacji, bez credentialu. Transport musi używać ID
ustalonego przez mTLS i ponawiać autoryzację działających streamów co najwyżej co 1 s.
`runtime_devices` jest odczytem administracyjnym hosta, również rekordów revoked;
handler zdalny musi wcześniej sprawdzić ADMIN. Zmiana zakresów przez
`update_runtime_device_authorization` stosuje CAS rewizji i atomowo odrzuca
nieuprawnione accepted, bez deklarowania cofnięcia efektów już attempting.

`RuntimeHost::identity_ref` wskazuje UUID manifestu tożsamości TLS w vault.
`set_runtime_identity_ref` jest lokalnym CAS referencji. Baza nie przechowuje
prywatnych kluczy hosta ani kopii certyfikatu hosta. Właściciel TLS tworzy vault
przed CAS i sprząta nieprzyjętą wersję; błąd odczytu istniejącej referencji nie
upoważnia do automatycznej regeneracji tożsamości.

`FileIdentity` zawiera kanoniczny absolutny root dostarczony przez host i leksykalnie
normalną ścieżkę względną; nie wykonuje I/O, także po usunięciu zasobu. Handler I/O
nadal odpowiada za rozwiązanie symlinków/reparse points pod tym rootem.
`set_runtime_dirty_presence` sprawdza FILES_GIT (8) i root oraz atomowo zapisuje
receipt i obecność właściciela: wyłącznie hash bazowy, czas hosta i rewizję, bez tekstu.
`runtime_file_presence` zwraca nazwę/ID urządzenia, hash, last_seen i rewizję również
po rozłączeniu lub restarcie. Brak TTL i automatycznego wygaszania. Hostowy odczyt
wymaga osobnej autoryzacji caller/root w handlerze zdalnym. Save/discard właściciela
czyści jego rekord; revoke również go usuwa. `dismiss_runtime_dirty_presence`
wymaga lokalnego hosta lub ADMIN i CAS rewizji; usuwa ostrzeżenie, nigdy zdalny draft.
Globalny licznik rewizji chroni przed starym Dismiss po usunięciu i ponownym dodaniu
presence. Retry tej samej komendy nie odświeża last_seen i nie zmienia rewizji.

`recover_runtime_chat` jest jawną komendą CHAT z CAS przyczyny odzyskiwania i epoki
historii. Wymaga istniejącego, włączonego agenta i jawnie wskazanego konta z katalogu;
dla ACP bez profili konto jest ID narzędzia. Nie wybiera profilu domyślnego.
Odzyskiwanie zachowuje chat.id, history epoch i wszystkie ID historii, usuwa tylko
recovery_reason i aktualizuje potwierdzony kontekst uruchomienia. Konkurencyjna
zmiana epoki lub drugi wybór odzyskania daje konflikt. Retry zwycięskiej komendy
zwraca zachowany receipt. Nowy zamiar odzyskania wymaga nowego command_id.

Powyższe rozszerzenia są częścią tej samej niewydanej `runtime.sql`, bez kolejnej
migracji etapowej. Ich regresje na tymczasowej bazie są w `runtime_presence`;
nie zastępują kwalifikacji mTLS na urządzeniu, interakcji UI ani macierzy natywnej.

`runtime_root_presence(root)` jest hostowym odczytem preflight dla całego
kanonicznego rootu: zwraca pary `(FileIdentity, DirtyPresence)` również dla offline
właścicieli i plików/worktree usuniętych z dysku. Używa indeksowanego zakresu
zakodowanej tożsamości, bez enumeracji filesystemu i bez dopasowania sąsiednich
rootów. Limit wynosi 1024 rekordy i 1 MiB oszacowanego wyniku; przekroczenie daje
`ResourceLimit`, nigdy częściową listę ryzyk. Handler zdalny nadal autoryzuje root
przed odczytem; preflight musi zachować błąd zamiast traktować go jako brak dirty.

Tożsamość katalogowa czatu (`ChatRecord::workspace`, serializowany `WorkspaceId`)
nie jest rootem uprawnień. Osobne `authority_root` przechowuje kanoniczny checkout
lub `private` dla czatu prywatnego. `with_authority_root` wymaga jawnej ścieżki
przy tworzeniu rozmowy projektowej; zapis i recovery odrzucają brak authority.
Komendy kolejki, zachowane receipts oraz artefakty czatu autoryzują tę ścieżkę,
nie identyfikator workspace. `SendContext::workspace` nadal identyfikuje katalog.

Adopcja wybiera `worktree_path` projektu (albo jego `path`) wyłącznie, gdy zapisany
katalog istnieje i jest już kanoniczny. Brak katalogu, niekanoniczna ścieżka lub
nieznany kontekst zachowują historię z `recovery_reason` i pustym authority.
Puste authority nie daje dostępu zdalnego nawet grantowi `*`; host musi jawnie
odzyskać kontekst. Scratch otrzymuje `private`. Adopcja zamkniętych czatów stosuje
tę samą regułę do zapisanego ClosedProject. Wszystko pozostaje częścią jednej
niewydanej migracji; nie ma automatycznego zgadywania nowego celu symlinka.

Nowy tekst Steer korzysta z `QueueDraft::command_for_steer` i tej samej atomowej
ścieżki przyjęcia `chat.send`. Opcjonalne `MessageIntent::steer_run` wiąże zamiar
z runem w fingerprintcie; brak pola zachowuje wcześniejsze kodowanie zwykłego
Send. Zmiana Send→Steer albo numeru run pod tym samym command_id daje konflikt.
`QueueEntry::steer_run` jest tylko wskazówką dispatch: właściciel runtime sprawdza
aktualny run przed i po przyjęciu, a stdin pozostaje niedostępne przed attempting.
Wskazówka jest zapisana w tej samej niewydanej migracji. Start hosta usuwa ją
atomowo z wstrzymaniem kolejek; retry zachowanego receipt jej nie przywraca.
Jawny Resend delivery_unknown tworzy nową zwykłą wiadomość bez wskazówki Steer.

## Natywne SettingsService i ArtifactsService

`remote::settings_rpc::SettingsRpc` uwierzytelnia certyfikat peer przez `RpcAuth`
i wymaga `ADMIN` oraz grantu `private` (lub `*`) także dla snapshotu. Odczyt zwraca
metadane, nazwy zmiennych i znaczniki configured; wartości env/credential nie mają
pola odpowiedzi. Typowany `SettingsHost` przekazuje patch do bieżącego właściciela
ustawień. Udany callback oznacza trwały zapis; nie wolno zastępować konfiguracji
kopią sprzed równoległej edycji. `expected_revision` jest częścią fingerprintu i trafia do każdej mutacji
`SettingsHost`; właściciel sprawdza ją atomowo razem z zastosowaniem patcha.
Brak rewizji oznacza patch bez warunku CAS, zachowujący pominięte pola.

Sekrety mają `Unchanged`, `Set` i `Remove`. Przed akceptacją `Set` tworzy niezmienną
wersję stagingu w osobnym namespace systemowego magazynu. Wersja wiąże host,
urządzenie, epokę, command ID, pole i HMAC treści. Fingerprint całej znormalizowanej
komendy również jest HMAC; SQLite nie przechowuje wartości ani prostego hasza
słabego sekretu. Klucz MAC pochodzi z utrwalonej tożsamości hosta. Brak klucza
blokuje zapis/retry zamiast generować nowy. Wstrzyknięty `SettingsSecrets` pozwala
testować te granice bez dostępu do prawdziwego magazynu systemowego.

Tabela `_canopy_settings_secrets` jest częścią tej samej niewydanej `runtime.sql`.
Rejestruje referencję przed operacją vault, dlatego crash pozostawia wykrywalny
staging. Właściciel konfiguracji odczytuje staging i używa istniejących API zapisu
profilu/integracji; nigdy nie zapisuje staging ID jako aktywnego credentialu.
`SettingsRpc::cleanup` usuwa wersje zakończonych komend oraz osierocone wpisy starsze
niż godzinę. `accepted`, `attempting` i `delivery_unknown` zachowują swoje wersje;
błąd cleanupu pozostawia wpis do ponowienia. Cleanup i staging korzystają ze
wspólnej bramki pojedynczej instancji serwisu. Host wywołuje cleanup okresowo.

RPC utrzymuje zaakceptowane wykonanie poza czasem życia żądania. `ResourceLocks`
ogranicza liczbę przyjętych operacji do 64 i zapewnia drain przed zamknięciem
runtime. Stan przechodzi accepted → attempting → succeeded/failed/delivery_unknown;
retry znanego wyniku nie wykonuje efektu ponownie. Błędy właściciela ustawień są
sanityzowane, aby nie odsyłać przypadkowo wartości formularza lub credentialu.

`remote::artifact_rpc::ArtifactsRpc` obsługuje BeginUpload, PutChunk, GetUpload,
FinishUpload, CancelUpload i strumieniowy Download. Begin wyznacza stabilny UUID
z hosta i tożsamości komendy. Ponowienie po utracie ACK odczytuje ten sam upload;
zmiana bindingu pod tym samym command ID daje konflikt. Finish/Cancel mają trwały
receipt; ponowienie Cancel działa również po usunięciu uploadu. Fragmenty nie
trafiają do inboxu: trwała kontrola offsetu i identycznych nakładających się bajtów
należy do magazynu. Strumień pobierania ma kolejkę jednego fragmentu 64 KiB,
sprawdza bieżące uprawnienia podczas odczytów i co sekundę oczekiwania; odbiorca
nieodbierający danych przez 30 s jest odłączany.

Dozwolone bindingi to Chat/Image, File/FileSave i Task/TaskAttachment. Task wymaga
pełnej referencji integration/project/task oraz `TASKS`, a globalny Tasks także
grantu `private`; sam workspace nie zastępuje TaskRef. Zakończenie uploadu jedynie
publikuje sprawdzony obiekt — zapis pliku, wysłanie obrazu i załącznik dostawcy są
osobnymi operacjami właścicieli. Limity magazynu pozostają 256 MiB, obrazu 64 MiB,
fragmentu 64 KiB i TTL 1 h dla niepodpiętych danych.

`ArtifactStore::import_chat_image` jest zaufanym hostowym mostem istniejącej historii:
odczytuje UUID.ext wyłącznie z hostowego `chat-attachments/<chat>`, sprawdza limit,
odrzuca symlinki pliku/katalogu czatu i wyznacza ID z czatu, nazwy oraz treści.
Publikacja wykorzystuje zwykły magazyn; osobna krótka transakcja zatrzymuje referencję
dla istniejącego czatu. Ponowna projekcja nie tworzy drugiego bloba.
`retained_image_metadata` służy do projekcji kolejki bez odczytu bajtów obrazu;
autoryzacja czatu nadal należy do handlera. `read_retained_image` weryfikuje treść
przed dispatch. Testy RPC, magazynu i stagingu nie stanowią odbioru GUI, platformowego
vault ani natywnych klientów.

Rozszerzone DTO zachowują znaczenie pominiętych pól: enabled/default profile,
model/argv profilu, opcje agenta, routing integracji i domyślne wybory czatu są
patchami. `Some("")` jawnie czyści default profile lub odpowiednią preferencję;
`None` zachowuje bieżącą wartość. `RemoveIntegration` przechodzi tę samą autoryzację,
CAS właściciela i lifecycle receipt co pozostałe mutacje. Read-only `Profile.settings`
nie jest komendą; zmiany trafiają przez `SaveProfileRequest.settings`.

System prompt i JSON override są osobnymi `SecretChange`, nigdy polami plaintext
inboxu. Limit każdego wynosi 64 KiB; staging dzieli UTF-8 na fragmenty do 2560 bajtów
bez rozcinania znaków, rejestruje każdy fragment w tej samej tabeli i składa wartość
wyłącznie na hoście. JSON musi być obiektem lub jawną pustą wartością. Credential
oraz pojedynczy env mają limit 2560 bajtów; pusty env jest poprawną wartością Set,
nie Remove. Ledger jest ograniczony do 8192 fragmentów. HTTP(S) endpoint/base URL
nie może mieć userinfo, query ani fragmentu; stara niebezpieczna wartość jest
pomijana w snapshotcie, a nowy zapis odrzucany.

Edycja kolejki zatrzymuje nowe `artifact_ids` w tej samej transakcji co payload,
rewizja i receipt. Nowe ID wymagają własności urządzenia, gotowego obrazu i
bindingu do dokładnie tego czatu. Niezmienione ID z poprzedniego draftu są już
stanem hosta: wystarcza istniejąca referencja obrazu tego czatu, także po revoke
pierwotnego nadawcy. Błędny nowy załącznik wycofuje cały edit wraz z retencją i
rewizją. Test `queue_edit_retains_new_images_atomically_and_keeps_revoked_senders_accepted_images`
obejmuje próbę wstrzyknięcia ID innego czatu oraz prawidłową edycję po revoke.

### Live remote settings owner

`app_state::remote::ControlBridge` applies administrative patches through the same
`SettingsState`, `ToolsState` and `IntegrationsState` that desktop preferences use.
A reply acknowledges the committed database result, not an enqueued action. A
single startup-random revision covers committed preferences, tools and integration
configuration; discovery/busy notifications do not change it. The app turn checks
CAS and reserves the owner before starting asynchronous persistence. An in-flight
write in any of these owners rejects a competing settings mutation as busy.

Remote environment, system-prompt and settings-JSON writes store immutable UUID
chunk references in the existing tool catalog, with values in the host credential
store. They do not persist staged IDs or plaintext values into SQLite. API keys use
the existing profile-key workflow. Launch resolves only the selected profile on a
background worker; missing chunks fail launch. Claude resolves overrides before
adding Canopy hooks. Retired chunk references use the existing Tools cleanup.
Legacy plaintext profile settings remain readable without a backfill. A local
editor changing a private field also writes a new immutable vault value; it never
downgrades an existing remote reference to plaintext SQLite. Unchanged legacy
fields stay readable, and unrelated profile edits preserve references and options.
Local forms mark vault-backed values as saved, preserve them when left empty,
and offer explicit removal; environment values can be replaced using the same name. Remote reads expose only
configured flags for private fields, and strip URL credentials, query and fragment.

Integration updates preserve omitted routing fields, verify the candidate account
through the existing provider adapter, then commit through the live integration
owner. As in desktop Connect, the stored login comes from provider verification. Removing an integration retires its active credential; the
existing account model requires a connected credential. Consequently a remote
credential `Remove` disconnects the account through the same owner path as
`RemoveIntegration`, preserving repository overrides and task links.

ACP profiles use explicitly named environment variables and command arguments.
Native Claude/Codex API-key, prompt/JSON and provider-option writes are rejected for
ACP rather than accepted and ignored; removal of legacy private fields remains
available. Runtime ACP option controls are separate from these host launch settings.


### Atomowe komendy czatu i fork

`get_runtime_command_matching` porównuje metodę, cel i fingerprint zamiaru po
sprawdzeniu bieżących grantów zapisanej komendy. Nie potrzebuje istniejącego
celu; zachowane `delivery_unknown` starej epoki podlega regułom GetCommand.
`CommandInput::with_fingerprint` pozwala handlerowi zachować hash zdekodowanego
zamiaru wire niezależnie od późniejszego rozwiązania zasobu.

`accept_runtime_chat_command` zapisuje accepted i referencje obrazów w jednej
transakcji IMMEDIATE. Nowy zamiar wymaga otwartego czatu bez recovery. Obrazy już
zatrzymane przez ten czat pozostają dostępne po revoke nadawcy i po fork; nowe
referencje wymagają bieżącej własności uploadu oraz właściwego celu i purpose.

`complete_runtime_fork(key, record, source_chat, replacement)` zatwierdza razem
nowy rekord, referencje obrazów źródła, opcjonalny derived Send i kolejkę oraz
wynik forka. Saved intent zawiera `source`, `history_epoch` oraz opcjonalny
`replacement_fingerprint`; brak fingerprintu zabrania dodania replacement,
a jego obecność zabrania pominięcia wiadomości. Epoka musi być bieżąca lub
zachowana w historii. Właściciel czatu nadal waliduje konkretny punkt forka i
kopiuje historię przed publikacją rekordu. Błąd transakcji nie publikuje czatu,
referencji, kolejki ani succeeded. Ponowienie sukcesu nie tworzy nowych efektów;
restart przywraca kolejkę jako wstrzymaną. Schemat pozostaje tą samą runtime.sql.

## Full-sharing defaults for paired computers

New host approvals default to all seven scopes and the `*` resource root, so
existing and subsequently opened projects, private chats and global resources
are included. The first runtime initialization with this policy atomically
upgrades only active, unchanged legacy defaults (`authorization_revision=1`,
Chat scopes, exactly `private`). The internal preferences key
`_canopy_remote_sharing_default` records completion in the same transaction.
Edited/custom grants and revoked devices are preserved, as are credential bytes,
device IDs, command epochs, receipts, queues and history. A failed write rolls
back both the grants and marker. No SQLite schema change is required.

Reciprocal desktop pairing uses stable peer device IDs and reuses an already
registered certificate. It never widens a later restricted grant or recreates a
revoked grant. Client-side mutual/pause flags are stored alongside the existing
connection; private key material remains in the OS credential store.
