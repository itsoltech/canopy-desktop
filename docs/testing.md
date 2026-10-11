# Testy i dobór weryfikacji

Priorytetem jest dowożenie kompletnej funkcjonalności. Liczba testów ani procent
pokrycia nie zastępują sprawdzenia zachowania, którego potrzebuje użytkownik.
Szersze uzupełnianie testów pozostaje osobnym etapem, wybieranym według ryzyka.

## Kiedy dodawać test

Najpierw wykorzystaj istniejącą ochronę. Nowy test ma wykrywać konkretny, istotny
błąd: naruszenie reguły domenowej, błędną transformację, regresję cyklu życia,
utratę danych lub przekroczenie granicy bezpieczeństwa. Przed dodaniem wskaż,
jaki błąd test wykrywa i dlaczego istniejące sprawdzenia go nie obejmują.
Nie dodawaj osobnego testu dla każdego typu, pola, helpera czy zmiany kosmetycznej.

Nie dodawaj:

- tautologii takich jak `assert 1 == 1`;
- testów potwierdzających jedynie, że DTO przechowuje przypisane pole;
- testów stałych i trywialnych getterów/setterów;
- asercji kopiujących implementację albo utrwalających prywatną strukturę UI;
- duplikatów pokrycia i testów mock-call counts bez znaczenia dla zachowania.

DTO służy do przenoszenia danych. Testuj rzeczywistą walidację, transformację lub
istotny kontrakt serializacji, jeśli istnieje, zamiast samego przypisania wartości.
Nie usuwaj wartościowych testów i nie ignoruj ich błędów w imię oszczędności.
Naprawiaj regresje wprowadzone przez zmianę. TDD stosuj tylko na wyraźne życzenie.

## Zakres sprawdzeń i zgoda

Po implementacji samodzielnie uruchamiaj adekwatne testy jednostkowe, lint
i formatowanie. E2E wymaga uprzedniej zgody użytkownika; najpierw ukończ
pozostałe dozwolone prace. Build, uruchomienie aplikacji, testy integracyjne
i operacje na usługach dobieraj do jawnie autoryzowanego zakresu sesji.
Nie powtarzaj pomyślnych sprawdzeń bez nowej zmiany, błędu lub nierozstrzygniętej
wątpliwości. Nie uruchamiaj ciężkiej pełnej suite'y dla drobnej poprawki.

Typowe polecenia lint/format: `cargo fmt --all -- --check` oraz
`cargo clippy --locked --all-targets -- -D warnings`. Testy dobieraj do zmienionego
zachowania; nie uruchamiaj przy okazji prawdziwego Keychain, sieci czy modeli.

Podaj wykonane sprawdzenia i ich wyniki oraz niewykonane kontrole. Testy modelu
i Clippy nie potwierdzają wyglądu, interakcji GUI, działania usług ani 120 FPS.
Procedury platformowe i wydajnościowe opisują [performance.md](performance.md)
i [kontrakty weryfikacji](project-contracts.md#weryfikacja-wydajność-i-praca-z-repo).
[verification.md](verification.md) zawiera historyczne wyniki, nie bieżącą gwarancję.
