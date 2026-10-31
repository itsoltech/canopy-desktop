# Wspólna magistrala komend

`src/command_bus.rs` realizuje pierwszy zakres BUS-01..04 i CMD-01..12 na
istniejącym workerze SQLite. Desktop i gRPC przekazują typowany zamiar do tego
samego właściciela walidacji, autoryzacji, fingerprintu i deduplikacji. Odczyty
i operacje wyłącznie widoku nie korzystają z inboxu.

## Wdrożony zakres

| Komenda rejestru | Efekt i wynik receipt |
| --- | --- |
| `chat.rename` | Zatwierdzona nazwa katalogowa i rewizja wątku |
| `chat.send` | Przyjęcie wiadomości do trwałej kolejki; ID wiadomości |
| `chat.queue.edit`, `move`, `remove`, `pause`, `resolve_unknown` | Atomowa zmiana kolejki i jej nowa rewizja |
| `settings.preferences` | Patch preferencji, pełne zatwierdzone wartości i wynikowa rewizja ustawień |

`Command`, `CommandEnvelope`, `Conditions`, `ResourceRef` i `Principal` nie są
typami Protobuf. `registry()` udostępnia nazwę, ryzyko i scope każdej komendy;
referencje plików workspace wymagają dodatkowo READ. Ryzyko jest punktem
rozszerzenia dla przyszłej polityki Ask. Obecny hook dopuszcza lokalnego właściciela
lub bieżące granty zdalnego urządzenia; nie dodaje dialogów potwierdzenia.
Lokalny właściciel jest kontekstem procesu, bez zdalnego credentialu. Principal
pochodzi z uwierzytelnienia adaptera, nigdy z pola żądania.

## Tożsamość, autoryzacja i retry

Pełny klucz inboxu to `(resource, executor_epoch, principal, command_id)`.
Dla wątku resource jest istniejącym `ThreadId`, a envelope zawiera `ThreadRef`
i epokę executora. Preferencje używają `EnvironmentRef` hosta. Rejestr przewiduje
też `ProjectRef`; w tym zakresie nie ma komend projektu. Niewdrożone komendy
zachowują namespace `resource=''`, `executor_epoch=0`. Epoka komend urządzenia
dalej chroni zamkniętą retencję; nie zastępuje epoki executora.

Ten sam UUID może niezależnie istnieć w dwóch wątkach, epokach executorów lub
u dwóch principals. Każdy odczyt, transition, ack i cleanup pojedynczego wpisu
używa pełnego klucza. Inbox nie ma cascade delete od katalogu wątków.
Trwały `thread_alias` pozwala odnaleźć receipt po usunięciu czatu.

Magistrala sprawdza bieżące uprawnienia przed ujawnieniem zapisanego wyniku
lub `idempotency_conflict` i ponownie przed dispatch. Sprawdza host, rodzaj celu,
epokę executora i warunki rewizji. Fingerprint SHA-256 obejmuje znormalizowaną
metodę, zasób, argumenty i warunki, bez bajtów Protobuf. Zapisany zamiar pozwala
odtworzyć te dane. Taki sam ID i zamiar zwracają zapisany stan; zmienione
argumenty lub warunki powodują konflikt.

Incoming draft zachowuje referencje załączników i podpisany kontekst. Rozwiązanie
workspace, kontekstu i retencji obrazów następuje po dedupe. Treść pliku nie jest
rozwijana do fingerprintu; zniknięcie zasobu po sukcesie nie zmienia receipt retry.

Legacy `GetCommand` i `ResolveCommand` nie zawierają resource ani epoki executora.
Po sprawdzeniu uprawnień odrzucają niejednoznaczny command ID, zamiast wybierać
dowolny wiersz. Odzyskanie wyniku: ponowić oryginalne typowane RPC z tym samym
command ID. Jawny retry Send zachowuje ID i zapisany request; nie stosuje hedgingu.
Protobuf pozostaje bez zmian.

## Zapis i niepewność dostarczenia

Wdrożone efekty są zmianami SQLite. Efekt domenowy i `succeeded` commitują się
w jednej transakcji (CMD-08). Nieudany zapis receipt wycofuje nazwę, preferencje,
kolejkę i retencję obrazów; nie wolno publikować sukcesu po samym enqueue zapisu.
Nie powstaje drugi inbox, worker ani warstwa repozytorium.

Sukces Send oznacza przyjęcie do kolejki, nie zakończenie wykonania. Istniejący
właściciel runtime osobno zapisuje `attempting` przed późniejszym dostarczeniem.
Restart/niepewna awaria pozostawia `delivery_unknown`, bez automatycznego replay.
Jawne resolve unknown zachowuje istniejące rozstrzygnięcia; Resend tworzy nową
pozycję kolejki atomowo w komendzie rozstrzygnięcia. Retry przyjętego Send nie
wysyła wiadomości ponownie ani nie wznawia kolejki wstrzymanej później.

Rename zatwierdza metadane przed publikacją w projektach i runtime. Opóźniony
zwykły save rekordu nie nadpisuje już jawnie zatwierdzonej nazwy.
Starszy lokalny pane bez katalogowej tożsamości przechodzi zwykły lazy save
katalogu przed rename; trwały alias jest odtąd używany ponownie. Zadanie lokalnej
komendy należy do ProjectsState i jest oczekiwane przy zamykaniu aplikacji.
Nazwy tabów ze splitami i etykiety lokalnych widoków zdalnych rozmów pozostają
stanem widoku; rename wątku hosta odbywa się przez jego komendę.

## Preferencje i wspólna rewizja

Lokalne `SettingsState::change`/`remember` oraz RPC UpdatePreferences używają
tego samego busa i właściciela rewizji w `SettingsState`. Rewizja nadal jest
losowa przy starcie i obejmuje zatwierdzone preferencje, narzędzia i integracje;
appearance, terminal, shortcuts i bookkeeping usuniętych credentiali są pomijane.
Rezerwacja właściciela obejmuje sprawdzenie CAS, commit i publikację. Konkurencyjny
zapis innego właściciela konfiguracji nie może zmienić tej granicy.

SQLite zapisuje patch i receipt z wartościami oraz wynikową rewizją w tej samej
transakcji. Retry zwraca dokładnie ten wynik, także po późniejszych zapisach
i restarcie; nie pobiera późniejszego snapshotu jako wyniku starej komendy.
Nieudany lub przestarzały CAS zachowuje wartości. Stan live jest publikowany
po commicie; zapamiętane lokalne zmiany są obsługiwane później.

## Schemat i pozostały zakres

Zmiany są częścią jedynej niewydanej `src/settings/database/runtime.sql` (D15).
Adopcja bazy sprzed runtime pozostaje transakcyjna. Prywatny prerelease o innym
fingerprincie nadal kończy się `UnsupportedSchema`; nie ma resetu ani automatycznej
aktualizacji niezgodnego schematu.

Poza magistralą pozostają create/delete/fork/recover, start/interrupt/restart,
steer/respond/options/agent/goal/background/edit/shell, otwieranie/zamykanie
projektów, terminale, Files/Git, Tasks/integracje, profile/sekrety, administracja
urządzeniami i artefaktami oraz inne mutacje. Ich dotychczasowe ścieżki pozostają
czynne. Sekrety nie trafiają do plaintext inboxu. Ten etap nie dodaje transportu
wątków, nowego API, MCP, outbox replay, replikacji ani handoff.
BUS-05/RPC-11 i porównanie deskryptorów są odroczone do `canopy.api.v1`.
