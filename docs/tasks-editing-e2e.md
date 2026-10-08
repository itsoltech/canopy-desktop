# Tasks editing — odbiór GUI/E2E przez użytkownika

Checklista przygotowana przez koordynatora 2026-09-10 na podstawie planu
[Tasks editing UX](tasks-editing-ux-plan.md). To scenariusze do wykonania,
nie raport przebytych testów. Koordynator nie przegląda kodu implementacji
i nie wykonuje GUI/E2E na prośbę użytkownika.

Do sprawdzenia zapisów użyj zadań testowych w każdej integracji. Test dotyczy
nowo zbudowanego i uruchomionego binarium; samo `cargo build --locked` nie
przełącza aktualnie działającej aplikacji ani nie buduje release `.app`.

## Codzienna edycja

| Scenariusz | Oczekiwany wynik | GitHub | Jira | YouTrack |
| --- | --- | --- | --- | --- |
| Otwórz istniejące zadanie z długim tytułem/opisem | Czytelny nagłówek, właściwy provider, widoczna stopka i najważniejsze właściwości | ☐ | ☐ | ☐ |
| Edytuj Summary i Description, przełącz Write/Preview | Tekst, kursor/draft i format opisu pozostają poprawne | ☐ | ☐ | ☐ |
| Wybierz osobę po nazwie/loginie | Dostępne osoby z właściwego projektu; użytkownik nie wpisuje accountId/ID | ☐ | ☐ | ☐ |
| Zmień pole z listy | Lista pokazuje nazwy, aktualny wybór, wyszukiwanie oraz legalne opcje providera | ☐ | ☐ | ☐ |
| Wybierz kilka etykiet/wartości, usuń jedną | Pozostałe wybory pozostają; prezentacja wartości i zaznaczenia listy są zgodne | ☐ | ☐ | ☐ |
| Wyczyść opcjonalną wartość | Jawne None/Unassigned usuwa tylko wskazaną wartość | ☐ | ☐ | ☐ |
| Zamknij formularz i otwórz ponownie | Niezapisany draft pozostaje, bez zapisu do serwisu | ☐ | ☐ | ☐ |
| Save changes i ponowne otwarcie | Widać dane przyjęte przez API; niezmienione właściwości pozostają | ☐ | ☐ | ☐ |
| Ponownie otwórz i wybierz Discard draft | Wraca stan bazowy właściwego zadania | ☐ | ☐ | ☐ |

Sprawdzaj tylko pola oferowane przez daną integrację. GitHub nie ma otrzymać
fikcyjnych sprintów/priorytetów, a ograniczenia projektu nie są brakującym
elementem formularza, jeśli są jasno komunikowane.

## Wspólny Status w szczegółach

- GitHub, Jira i YouTrack mają w szczegółach tę samą kontrolkę Status. Nie ma
  osobnych przycisków Close/Reopen/Change status. Wybór uruchamia zmianę bez Save;
  wyjątkiem jest wymagający pól formularz przejścia Jira opisany poniżej.
- Sprawdź obsługę strzałek/Enter, loading, blokadę podwójnego zapisu i zachowanie
  ostatniej potwierdzonej wartości do odpowiedzi API. Zamknięcie szczegółów nie
  anuluje zapisu; zmiana konta/workspace nie przyjmuje starego wyniku.
- Refresh wyłącznie odczytuje. Po niepewnym zapisie kolejna zmiana wymaga Refresh.
  Przyjęty zapis z błędem odświeżenia pozostaje sukcesem z ostrzeżeniem.

## Różnice między integracjami

- GitHub: dropdown Status zawiera Open/Closed (nie statusy GitHub Projects).
  Sprawdź zamknięcie, ponowne otwarcie i brak zapisu po wyborze bieżącej wartości.
- GitHub: labels, assignees i milestone w formularzu oraz szybkich akcjach.
  Milestone pojedynczy, labels/assignees wielokrotne. Sprawdź aktualny milestone
  lub label spoza pierwszej strony listy. Przy kilku zapisach błąd części operacji
  nie może udawać pełnego sukcesu ani cofnięcia zmian już przyjętych przez GitHub.
- Jira: assignee bez surowego accountId, priority/type, components/fix versions,
  parent po kluczu lub tytule; cascading custom field jako parent/child.
  Dropdown Status pokazuje legalne transitions. Bez wymaganych pól zapis jest
  natychmiastowy; wymagane pole (również z defaultem) otwiera formularz wskazanego
  przejścia. Anulowanie nie wysyła zmiany. Drafty dwóch przejść nie mieszają się;
  zniknięcie wybranego przejścia nie wybiera innego. Sprawdź dwie akcje do tego
  samego statusu, brak dostępnych przejść oraz zmianę wymagań tuż przed zapisem.
  Zwykła edycja nie zawiera statusu ani nie przywraca go ze starego draftu.
  Sprint korzysta z wyboru board/sprint; bogaty niezmieniony opis ADF pozostaje
  nienaruszony. Sprawdź typ zadania zmieniający wymagane pola.
- YouTrack: Status zmieniaj dropdownem w szczegółach, bez otwierania edycji i bez
  Save. Wybór wysyła jedną zmianę; podczas zapisu kontrolka jest zablokowana.
  Powtórny wybór bieżącego statusu nie wysyła zapisu. Przy workflow lista zawiera
  dozwolone akcje, a po sukcesie pokazuje wynikowy status z API, nie nazwę akcji.
  Sprawdź błąd uprawnień/workflow, niepewny zapis, Refresh po błędzie odczytu oraz
  zamknięcie szczegółów podczas zapisu. Nie testuj mutacji na zadaniach produkcyjnych.
- YouTrack: edycja istniejącego zadania zawiera pozostałe pola (Type, Priority,
  Sprint/Version oraz Assignee stosownie do schematu), bez statusu. Stary draft
  statusu nie może cofnąć zmiany ze szczegółów; Create nadal używa pól wymaganych
  przy tworzeniu. Listy nadal istnieją po pobraniu szczegółów zadania i po
  otwarciu draftu. Tagi wybiera się po nazwie. Sprawdź single/multi custom field,
  wymagany i read-only field oraz dostępne zdarzenia state machine, jeśli projekt
  go używa. Adres YouTrack nie jest podpisany `JIRA SITE`.

## Trudniejsze stany

- Lista z wieloma stronami: doładuj, wyszukaj wartość spoza pierwszej strony,
  wybierz ją, zamknij i otwórz picker. Wartość i jej nazwa pozostają zgodne.
- Dwie opcje o tej samej nazwie: da się je rozróżnić, wybór nie przeskakuje
  po sortowaniu/odświeżeniu. Aktualna niedostępna lub zarchiwizowana wartość
  pozostaje widoczna i nie jest automatycznie czyszczona.
- Puste wyniki wyszukiwania różnią się od braku opcji i od błędu połączenia.
  Błąd ma Retry; nie zamienia kontrolki w input z ID ani nie usuwa wyborów.
- Przy niedostępnym połączeniu lub odmowie uprawnień Save pozostawia draft
  i wskazuje problem. Ponowienie odczytu nie wysyła mutacji samoistnie.
- Szybko zmieniaj zapytanie i przełącz zadanie/projekt w trakcie ładowania:
  stara odpowiedź nie może nadpisać nowego kontekstu.
- Niepoprawna liczba/data albo brak wymaganego pola: czytelny błąd przy polu,
  także gdy należałoby ono do More fields. Pozostały draft zostaje.
- Dwa szybkie kliknięcia Save: jedna operacja, stabilny loading button.
- Normalne zamknięcie i restart aplikacji z draftem: draft wraca do właściwego
  konta/projektu/zadania. Nie używać force kill jako testu gwarantowanej trwałości.

## Layout, klawiatura i motion

- Normalne i małe okno: formularz mieści się, siatka przechodzi do jednej kolumny,
  długie nazwy nie wypychają kontrolek, stopka pozostaje dostępna.
- Scroll długiego formularza/opisu; dropdown przy dolnej krawędzi: lista nie
  jest obcięta, nie przewija przypadkowo tła i nie znika przy granicy scrolla.
- Tab / Shift+Tab, strzałki, Enter; Escape zamyka najpierw listę, potem modal,
  zachowuje draft i przywraca sensowny fokus.
- Multiselect Jira/YouTrack: wybrane nazwy wyłącznie w kontrolce, bez osobnych
  chipów nad nią. Odznaczenie jednej pozycji na liście zachowuje pozostałe.
- More fields z wieloma wierszami i długimi opisami pomocniczymi: po rozwinięciu
  przewiń do ostatniej kontrolki. Wszystkie pola mają pełną wysokość; sekcja
  nie ma własnego scrolla ani limitu wyliczanego z liczby pól. Powtórz po resize.
- Szybkie otwarcie/zamknięcie pickera i More fields: ciągłe wejście/wyjście,
  brak skoków i przycisków reagujących po zamknięciu.
- Hover in/out kontrolek z tooltipami: brak crasha lub utraty stanu hover.
- Loading opcji nie animuje od nowa całego modala. Zapis nie zmienia skokowo
  szerokości przycisku i nie pozwala na drugi submit.
- Reduce Motion: operacje działają bez opóźnionych przejść.
- Kliknięcie/scroll poza modalem i podczas jego wyjścia nie działa na tle.

## Zgłoszenie regresji

Wystarczy podać: provider, rodzaj pola, kroki, oczekiwany i uzyskany efekt,
czy problem dotyczy istniejącego zadania czy tworzenia, oraz zrzut UI.
Do błędów danych przydatne są nazwa/typ pola i informacja o single/multi;
nie trzeba podawać tokenów ani danych logowania.
