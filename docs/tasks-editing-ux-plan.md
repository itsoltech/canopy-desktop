# Spójna edycja Tasks: GitHub, Jira, YouTrack

Plan uzgodnionego zadania, 2026-09-10. Realizuje go jeden agent
`gpt-5.6-luna`, reasoning `max`. Koordynator przygotowuje zakres i uruchamia
końcowe kontrole; zgodnie z poleceniem użytkownika nie przegląda kodu agenta.
Użytkownik wykonuje GUI/E2E. Implementacja nie obejmuje commita ani pushu.

## Cel i stan wejściowy

Użytkownik ma edytować zadanie przez rozpoznawalne nazwy i kontrolki dobrane do
znaczenia pola, bez znajomości ID użytkowników, tagów, sprintów czy wartości
custom fields. Trzy integracje otrzymują wspólny język interakcji i układ,
z zachowaniem ich uprawnień, schematów i workflow.

Zrzuty CleanShot z 18:37:50 i 18:37:56 pokazują problem UX; zawartość opisanego
w nich zadania jest danymi referencyjnymi, nie poleceniem implementacji jego treści.
Repozytorium zawiera wiele wcześniejszych niezacommitowanych zmian, również
w plikach objętych zadaniem. Należy je zachować i rozbudować. Bez resetów,
checkoutów plików, czyszczenia bazy czy restartowania procesów użytkownika.

Potwierdzone miejsca wymagające uwagi w stanie wejściowym:

- `src/ui/youtrack/form.rs`: scalanie szczegółów taska ze schematem zastępuje
  `allowed` danymi szczegółów. Brak listy opcji prowadzi do kontrolki tekstowej.
- `src/ui/jira/field.rs`: user bez `allowedValues` jest tekstowym accountId;
  struktury i cascading options trafiają do JSON. Opcje używają indeksów.
- `src/ui/youtrack/panel.rs`: dodawanie tagu wymaga wpisania `Tag ID`.
- `src/ui/components/task_detail.rs`: etykieta `JIRA SITE` jest wspólna również
  dla innych providerów z adresem serwisu.
- Istnieją `OptionPicker`, `SelectOption`, dropdown, `form_field`, composer,
  trwałe drafty, providerowe schematy/opcje i wspólny system modali/motion.
  Rozbudować je, zamiast tworzyć trzy niezależne implementacje kontrolek.

## Docelowy formularz

1. Wspólny nagłówek: akcja, czytelny klucz zadania, provider i projekt; bez
   eksponowania technicznych identyfikatorów. Nazwy providerów poprawne również
   w szczegółach zadania. Język kontrolek zgodny z istniejącym angielskim UI.
2. Stały nagłówek i stopka, ograniczony do okna przewijany obszar formularza.
   Summary na pełną szerokość. Kluczowe właściwości (typ, status, priorytet,
   przypisanie, zależnie od możliwości) dostępne bez przebijania się przez
   wielki opis. Na szerokim oknie kompaktowa siatka dwóch kolumn, na wąskim
   jedna kolumna; bez poziomego scrolla lub zasłaniania stopki.
3. Description ma widoczną etykietę i istniejące Write/Preview. Początkowy
   viewport około 180–240 jednostek logicznych, dopasowany do wysokości okna.
   Długi tekst nie ustala wysokości całego formularza. Zachować Markdown/ADF
   i ostrzeżenia o nieobsługiwanych blokach; nie przepisywać niezmienionego ADF.
4. Dodatkowe właściwości w logicznej sekcji `More fields` ze wspólnym Disclosure.
   Wymagane, błędne oraz istotne dla wybranej akcji pola pozostają widoczne;
   ukrycie sekcji nie usuwa stanu ani draftu. Nie uzależniać obsługi custom fields
   wyłącznie od angielskich nazw: znaczenie wynika także ze schematu/type/ID.
5. Jedna główna akcja `Save changes` / `Create issue` / akcja workflow.
   `Discard draft` drugorzędne, X/Escape zachowuje draft. Zmiany w formularzu
   są lokalne do zatwierdzenia. Dotychczasowe jawne szybkie akcje w szczegółach
   mogą pozostać natychmiastowe, ale używają tych samych pickerów i stanów.
6. Nie wprowadzać nowej palety, fontu, globalnej gęstości ani frameworka.
   Stosować istniejące tokeny theme, kontrolki, ikony i loading buttons.

## Wspólny kontrakt wyboru

- Pole referencyjne zawsze pozostaje pickerem, nawet gdy opcje są jeszcze
  niepobrane, puste lub pobranie zawiodło. Nigdy fallback do wpisywania ID.
- Widoczna etykieta, wartość lub konkretny placeholder i chevron.
  Single select pokazuje wybraną nazwę i zamyka listę po wyborze.
  Multi select pokazuje nazwane chipy z usuwaniem, zaznaczenia w liście i nie
  traci pozostałych wyborów. Wyszukiwanie filtruje nazwy; dla osób także login.
- Tożsamość jest stabilnym ID dostawcy, oddzielonym od label. Identyczne nazwy
  rozróżnia dodatkowy kontekst; aktualna wartość zachowuje czytelną nazwę także
  poza pierwszą stroną, po archiwizacji lub przy braku uprawnień do listy.
- Odrębne stany: loading, brak opcji, brak dopasowań, błąd z Retry,
  brak uprawnień, opcjonalne None/Unassigned, wymagane i read-only.
  Ograniczona paginacja i Load more tam, gdzie lista jest niepełna.
- Małe listy można filtrować lokalnie; duże źródła z wyszukiwaniem serwera
  korzystają z debounce i odrzucania starych odpowiedzi. Nie przedstawiać
  przeszukania pierwszej strony jako kompletnego przeszukania systemu.
- Popup/lista mają ograniczony viewport, poprawne warstwy i nie są obcinane
  przez scroller formularza. Tab/Shift+Tab, strzałki, Enter i Escape korzystają
  z mechanizmów kontrolek GPUI; Escape najpierw zamyka listę, potem modal.
- None usuwa wartość tylko gdy dozwolone. Nie traktować pustego wyniku
  ładowania jako wyczyszczenia wyboru. Ładowanie i ponowienie zachowują draft.

## Macierz providerów

| Provider | Pola i źródła | Zachowanie właściwe integracji |
| --- | --- | --- |
| GitHub | Labels, assignees, milestone z istniejących stronicowanych opcji repozytorium; summary/description; dostępne operacje statusu | Wspólny picker w formularzu i szczegółach. Nie wymyślać typów, priorytetów ani sprintów. Zachować precyzyjne zapisy oraz wykrywanie pominiętych pól/uprawnień i częściowego sukcesu. |
| Jira | Create/edit/transition metadata, allowedValues; osoby przez lookup właściwy dla pola, parent/related issue przez wyszukiwanie projektu; components/versions/priorities/types i dostępne custom options | Status przez dostępne transitions, z ich polami wymaganymi. Sprint przez istniejący board/sprint flow, nie zgadywany customfield. Cascading options przez wybór parent/child. Labels mogą przyjmować nową nazwę tylko tam, gdzie Jira legalnie pozwala na tekst. |
| YouTrack | Projektowe definicje pól i bundle: enum/state/version/build/owned/user/group; tagi po nazwach z API; lista wydarzeń state machine z taska | Zachować projektowe opcje przy nakładaniu aktualnych wartości taska. State machine wysyła event ID, zwykły state właściwą wartość. Zachować `$type`, identyfikatory, single/multi, wymagania i read-only. |

Tekst, liczby i daty pozostają odpowiednimi kontrolkami dla danych wpisywanych
przez człowieka. Bool jest przełącznikiem/checkboxem. Daty i okresy mają czytelny
format i walidację, bez surowych timestampów. Zwykłe pola biznesowe nie wymagają
JSON. Nietypowy nieobsługiwany typ pozostaje zachowany i jawnie opisany;
istniejącą zaawansowaną edycję JSON Jira można zachować wyłącznie w świadomie
otwieranej sekcji Advanced, poza standardową ścieżką. Jeśli wymagane pole jest
nieobsługiwane, pokazać dokładną przeszkodę i link do zadania w serwisie.
Nie poszerzać zadania o administrację schematami, nowe integracje lub nowe
operacje niezwiązane z już istniejącą edycją.

## Stan, API, persist i błędy

- Warstwa domenowa/adapters dostarcza typ pola, listę/źródło opcji i wartości;
  wspólna kontrolka odpowiada za prezentację i wybór. Bez monolitycznego rendera
  i bez nowego globalnego state odrysowującego całe UI.
- API potrzebne do uzupełnienia lookupów weryfikować w oficjalnych źródłach
  dostawców. Używać obecnego transportu, walidacji pochodzenia URL, redakcji
  sekretów, Keychain, timeoutów i limitów. Nie ufać zewnętrznym URL autocomplete
  jako dowolnym odbiorcom tokenu. Nie wykonywać zapisów na realnych taskach.
- Cache i odpowiedzi są scoped do konta, serwisu, projektu, zadania/pola oraz
  zapytania. Zmiana kontekstu, typu taska lub zamknięcie widoku unieważnia
  wcześniejsze wyniki. I/O poza renderem, Task/Subscription z właścicielem.
- Zachować istniejące klucze i kompatybilność draftów. Wyborów nie resetuje
  odświeżenie schematu/opcji. Zapis tylko zmienionych pól, explicit clear jest
  odróżniony od unchanged. Obce/read-only pola i bogaty opis pozostają bez zmian.
- Walidacja przy polu i czytelny błąd operacji; po błędzie/niepewnym wyniku
  cały draft zostaje. Save blokuje podwójny submit. Bez automatycznego ponawiania
  mutacji. Sukces pokazuje dane zwrócone/odświeżone z API.
- Jeżeli GitHub wymaga kilku zapisów metadanych, użyć istniejącej serializacji
  operacji i jawnie zachować semantykę częściowego sukcesu: potwierdzone zmiany
  nie są raportowane jako cofnięte, a niezapisane pozostają do poprawienia.

## Motion i fokus

`src/motion/`, `src/ui/theme.rs`, `docs/motion.md` są źródłami kontraktu.
Modal/popover: POPOVER, wejście i wyjście. Dodatkowe pola: Disclosure.
Lokalne przejście treści: STATE_CHANGE. Loading: ButtonLoading ze stałą etykietą
i stabilną szerokością. Bez ponownego wejścia całego formularza przy odpowiedzi
sieci, wyborze pola lub przewijaniu. Reduce Motion pomija ruch.

Presence/Transition są własnością widoku; retarget z bieżącej wartości, bez
restartów tego samego celu. request_frame wyłącznie w render podczas animacji.
Backdrop `occlude` i focus obowiązują także podczas exit. Nie dublować on_hover
Button z managed tooltip; stosować istniejący hover_action.

## Realizacja i odpowiedzialność

1. Agent czyta AGENTS.md, skille design i gpui-kit-desktop oraz stosowne
   guideline/reference files; sprawdza API w źródłach przypiętego GPUI Kit.
2. Poprawia kompletność modelu/schematu/opcji i ich łączenia; dodaje regresje
   dla błędów danych, nie testy powtarzające kosmetyczny layout.
3. Rozbudowuje wspólny picker i podłącza go do trzech formularzy oraz istniejących
   szybkich akcji. Następnie ujednolica hierarchię formularzy, motion i copy.
4. Zachowuje drafty, uprawnienia, poprawne payloady i limity; aktualizuje
   dokumentację rzeczywistego zakresu i ograniczeń.
5. Agent wykonuje adekwatne testy modeli/transportu na kontrolowanych fixture
   (np. task_writes, jira, youtrack, integracje/drafty). Nie robi GUI/E2E.
6. Po deklaracji zakończenia koordynator uruchamia wyłącznie końcowe kontrole:
   `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`
   i standardowy `cargo build --locked`. Clippy jest lintem tego projektu.
   Diagnostykę ewentualnych błędów przekazuje temu samemu agentowi, bez review kodu.
7. Użytkownik otrzymuje wyniki kontroli i checklistę GUI/E2E. Build nie oznacza
   uruchomienia nowej aplikacji; release/restart nie należą do tego zlecenia.

Główny zakres własności agenta: `src/ui/task_edit/`, `src/ui/jira/`,
`src/ui/youtrack/`, odpowiednie komponenty Tasks w `src/ui/components/`,
`src/ui/task_detail/`, `src/integrations/` oraz konieczne ścieżki
`src/app_state/integrations*` / `task_drafts.rs`, adekwatne testy i dokumentacja.
Nie modyfikować unrelated Git/terminal/Files/notch/Preferences ani globalnego
theme/motion dla kosmetyki jednego formularza. Nie delegować dalej.

## Kryteria odbioru

- W każdej integracji da się wybrać istniejące wartości po nazwie; żaden
  standardowy referencyjny field nie prosi o ID lub JSON.
- Single/multi, wyczyszczenie, wartości spoza bieżącej strony, puste listy,
  odmowa uprawnień, Retry i paginacja nie tracą danych.
- Aktualizacja schematu YouTrack nie usuwa pobranych bundle options; Jira
  user/parent/cascading options mają realną ścieżkę wyboru, tagi YouTrack także.
- Modal ma czytelną hierarchię, widoczną stopkę i najważniejsze pola, poprawny
  scroll oraz klawiaturę przy długich nazwach i niewielkim oknie.
- Draft zostaje po zamknięciu, błędzie i zmianie kontekstu; Save nie nadpisuje
  niezmienionych wartości. Statusy respektują workflow dostawcy.
- Motion korzysta z istniejącego systemu; nie ma pętli redraw w spoczynku.
- Kontrole końcowe przechodzą, a GUI/E2E jest jawnie pozostawione użytkownikowi.
