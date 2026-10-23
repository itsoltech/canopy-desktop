# Samodzielne rozmowy

## Cel i zakres

Rozmowa bez projektu jest bezpośrednim celem nawigacji, inspirowanym wątkami
T3 Code. Sidebar pokazuje `Chats` nad `Projects`; nie pokazuje sztucznego projektu
ani worktree `No project`. Projekty zachowują dotychczasową hierarchię.

Chats pokazuje rozmowy ze wszystkich otwartych workspace'ów, projektowych i
samodzielnych. Jeden wiersz to jeden `PaneKind::Chat`, identyfikowany przez `PaneId`.
Źródłem jest `ProjectsState.runtime_workspaces`: autorytatywna aktywna encja albo
cache projektu/scratch. Nie ma migracji danych ani dodatkowego katalogu rozmów.
Kolejność wynika z katalogu workspace'ów oraz kolejności tabów i liści,
nie z bieżącej aktywności agenta.

Wiersz ma trzy poziomy, wzorowane na sidebarze T3 Code: projekt i status,
tytuł rozmowy, następnie branch/worktree i nazwa agenta z ikoną. Projekt pochodzi
z właściciela workspace'u, agent z `Pane.tool`, a HEAD z konkretnej ścieżki
checkoutu w istniejącym watcherze Git. Nie dziedziczy brancha aktywnego projektu
ani głównego checkoutu. Linked worktree pokazuje także nazwę katalogu; tooltip
zawiera pełną ścieżkę i HEAD. Długie etykiety są skracane, agent ma własne miejsce.
Rozmowy samodzielne pokazują `Without a project` i `Private folder`. Folder bez
Git, ładowanie/błąd metadanych oraz detached/unborn HEAD mają jawne etykiety.
Obserwacja Git, workspace, projektów i agentów odświeża dane bez uruchamiania
nieodwiedzonej rozmowy. Render nie odczytuje systemu plików.
Kontekst projektu ma turkusowy akcent, a prywatny czat fioletowy. Ikony Claude
i Codex mają własne kolory; status ma kropkę i nazwę (Idle zielony, Working
niebieski, uwaga żółta, błąd czerwony). Wybrany wiersz ma delikatnie niebieskie
tło i pasek. Tytuł jest mocniejszy od metadanych; trzy linie mają równe odstępy,
stałą interlinię, symetryczne pionowe paddingi i większy odstęp ikona–etykieta.
Tokeny kolorów i geometrii są w `src/ui/theme.rs`.

Pasek tabów, splity i dock pozostają dostępne. `Open tabs…` w sekcji Chats
zapewnia powrót do tabów bez czatu, np. podglądu pliku, także z aktywnego projektu.
Nie wprowadzamy archiwum: `Delete chat` usuwa rozmowę i jej załączniki.

## Tworzenie i nawigacja

- `New chat` i ⌘N tworzą rozmowę w aktualnie wybranym projekcie/worktree,
  z jego cwd. Bez aktywnego projektu tworzą prywatną rozmowę.
- `New private chat` w menu i ⌥⌘N zawsze tworzą rozmowę bez projektu.
  Tooltip głównego przycisku wskazuje bieżący cel.
- Menu `New chat in…` i ⌘⇧N otwierają istniejącą paletę wyboru kontekstu
  przed utworzeniem rozmowy. Fork nadal korzysta z tej palety.
- Windows zachowuje odpowiedniki skrótów z kontekstu Workspace.
- Start aplikacji, powrót do rozmów i zamknięcie ostatniego projektu nie tworzą
  automatycznie rozmowy. Pusty stan ma jawne CTA.
- Kliknięcie wiersza aktywuje aktualny tab i pane; rename/delete nie zmieniają
  widocznego projektu. Operacje ponownie odnajdują cel po stabilnych ID.
- Menu rozmowy jest dostępne klawiaturą i bez hover. Status ma nazwę tekstową;
  odczyt statusu nie materializuje widoku ani nie uruchamia agenta.

## Tytuły

Efektywny tytuł należy do modelu Workspace: `PaneMetadata.title`, z fallbackiem
do istniejącej nazwy taba dla starszych zapisów. Nieplaceholderowy fallback
chroni nazwę przed automatycznym nadaniem po następnej wiadomości.
Odczyt nie przepisuje danych. Rename zmienia wyłącznie wybrany czat; dla
pojedynczego pane'a synchronizuje też tab. Nazwa taba ze splitem jest nazwą grupy.
Opcjonalny `PaneMetadata.chat_title_state` odróżnia nowy placeholder od nadanej
nazwy (także ręcznie nadanego `New chat`). Brak pola zachowuje odczyt starszych
snapshotów; nie zmieniamy schematu SQL.
Detach zachowuje nazwę rozmowy. Model, żywy ChatView i sesja agenta używają tej
samej nazwy, również w notchu, forku i podglądzie zmian.
Przed dock, swap, detach i zmianą nazwy grupy utrwalamy odziedziczony tytuł
dotkniętych czatów na kandydacie operacji. Nieudana operacja nie zmienia modelu.

## Usuwanie i odpowiedzialność

`ProjectsState` posiada adresowane operacje na rozmowach w każdym workspace, także
nieaktywnym projekcie lub scratch. Rename/Delete nie przełączają widocznego projektu.
`Chats` posiada guard,
rejestr metadanych wszystkich czatów i operacje cleanup, także bez `ChatView`.
Sidebar posiada tylko stan interakcji i subskrypcje. Modale zachowują istniejącego
właściciela Workspace. Render nie wykonuje I/O.

Kolejność usuwania:

1. Sprawdzenie celu i zależności prywatnego katalogu.
2. Potwierdzenie nazwanej rozmowy przez wspólny guard edytorów/czatów.
   Nieznana, ładowana lub nieodczytana historia jest traktowana ostrożnie.
3. Ponowne sprawdzenie ID, rodzaju pane'a i zależności; odmowa nie rusza kolejki.
4. Usunięcie wyłącznie pane'a; zwykłe uproszczenie drzewa zachowuje sąsiadów.
5. Zablokowanie dalszego zapisu historii i rezerwacja katalogu na czas cleanup.
6. Potwierdzone zakończenie agenta, startu w toku i poleceń `!`.
7. Ponowna kontrola użycia katalogu i odebrane wyniki usuwania katalogu,
   historii SQLite i załączników.

Operacja jest jedna na PaneId; Stop i Quit dołączają do niej. Błąd zachowuje
właściciela cleanup i możliwość ponowienia, a sukces innej operacji nie usuwa
tego błędu. Quit czeka także na cleanup bez widoku. Nie obiecujemy rollbacku
częściowego kasowania; ponowienie kończy pozostałe idempotentne etapy.
Naturalne zakończenie shella `!` nie oznacza zakończenia jego potomków.
Uchwyt grupy albo informacja o niepotwierdzonym cleanup pozostaje we właścicielu
rozmowy do potwierdzenia zwolnienia zasobów; nie sygnalizujemy starego PID.

Katalog używany przez inny pane lub kończący się proces jest chroniony.
Użytkownik dostaje nazwę zależnego widoku do zamknięcia; nic nie jest zamykane
po cichu. Sprawdzenie obejmuje wszystkie workspace'y i rejestry zamykanych
procesów. Nowe otwarcia w katalogu zarezerwowanym do usunięcia są odrzucane.
Prywatna ścieżka pozostaje zarezerwowana do końca sesji aplikacji także po
cleanup, aby spóźniony wynik otwarcia pliku nie odtworzył usuniętego katalogu UUID.
Kanonizacja i fizyczne usuwanie pozostają w workerze, z ograniczeniami
`chat::scratch::remove`. Nie wprowadzamy trwałych lease ani garbage collectora.

## Kontekst okna

Bez projektu Files i Tools są schowane, z zachowaniem encji i preferencji.
Session, globalne Tasks i notch pozostają dostępne. Changes i transport Git
są niedostępne. Efektywny fallback do Session nie nadpisuje preferencji
inspektora ani jego szerokości. Tytuł okna pochodzi z rozmowy/taba albo `Chats`.
New tab bez projektu tworzy czat; skrót splitu nie tworzy terminala.

## Weryfikacja

Istotne regresje: rename/delete nieaktywnego scratch, zakres jednego pane'a,
tytuły legacy i ręczne po restore, detach, guard bez załadowanego widoku,
współdzielony katalog, odrzucona operacja bez utraty kolejki, cleanup i Quit
w trakcie zamykania. Wykorzystujemy istniejące testy workspace/session oraz
dobrane testy najniższej warstwy. Test interakcji sidebara sprawdza projekt,
branch/worktree i agenta rozmowy projektowej oraz samodzielnej, odświeżenie HEAD,
klik/menu/Escape i geometrię przy szerokościach 220/320 oraz czcionce 16/20.
Sprawdza także równe odstępy między liniami i pionowe paddingi. Regresja New chat
sprawdza utworzenie pane'a i cwd po wyborze głównego checkoutu oraz linked worktree,
bez dodawania prywatnej rozmowy.
Operacje nieaktywnego projektu zachowują bieżący projekt i sąsiedni pane.
Cargo z `--locked`, fmt i Clippy.
GUI/E2E i prawdziwe modele wymagają osobnej weryfikacji; testy modelu nie
potwierdzają wyglądu ani interakcji w natywnym oknie.
