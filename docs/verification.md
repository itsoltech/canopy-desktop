# Układ workspace'u i odbiór wizualny — 2026-10-23

Środowisko: Linux x86_64, Rust 1.95.0, GPUI Kit 0.7.1, przypięty Cargo.lock.
Toolchain, biblioteki i ekran Xvfb z programowym Vulkanem przygotowano w
`~/.cache/canopy-ui-verification`, bez zmiany globalnej konfiguracji.

Wyniki sprawdzeń:

- `cargo test --locked --bin canopy-desktop ui:: -- --nocapture` — 59 zaliczonych,
  2 ignorowane. Obejmuje m.in. menu, powrót do rozmowy, układ paneli, przewijanie
  kart i fokus nakładki sidebara.
- `cargo test --locked --test workspace --test session` — 16 + 11 zaliczonych.
  Obejmuje m.in. niezależność wyboru od fokusu, powrót A + shell S → CLI B → A,
  pierwszy czat po CLI oraz persist/restore.
- Filtry `app_state::projects` i `app_state::agents` w testach binarium — 5 + 1
  zaliczonych. Łącznie 92 zaliczone testy.
- `cargo fmt --all -- --check`, `git diff --check` i
  `cargo clippy --locked --lib -- -D warnings` — OK.
- Pełny Clippy z `-D warnings` nadal zatrzymują wcześniejsze ostrzeżenia
  platformowe: nieużywany kod w attachment/media/notch/html_native,
  `drop_non_drop` w chat/page i `needless_borrows_for_generic_args` w chat/quote.
  Kontrola wszystkich targetów przechodzi po lokalnym dopuszczeniu wyłącznie
  kategorii `unused_variables`, `dead_code`, `clippy::drop_non_drop` i
  `clippy::needless_borrows_for_generic_args`. Nie dodano wyciszeń do kodu.

Odbiór natywny korzystał z jawnie ignorowanego testu
`ui::visual_tests::native_visual_fixture`: produkcyjne widoki, syntetyczne
projekty/sesje i deterministyczny bufor terminala. Tekst terminala w tym fixture
jest przykładem renderowania, a nie dodatkowym wynikiem testów. Nie uruchamiano
PTY, modeli, kont ani usług zewnętrznych.

Obejrzano puste worktree, rozmowę, CLI, przepełniony pasek kart, Files, boczną
nakładkę oraz menu tworzenia rozmowy, narzędzi, projektu, sesji i wszystkich
kart. Macierz: 1440×900 przy bazie 16, 1280×800 przy bazach 18/20 oraz 800×740
przy bazie 20. W natywnym oknie wykonano kliknięcia menu, przełączanie kategorii,
Escape i obsługę klawiatury nakładki; test GPUI sprawdza przejęcie i przywracanie
fokusu. Zrzuty i logi pozostawiono w `~/.cache/canopy-ui-verification`.

Review Astry wykryło regresje wyboru/fokusu, pierwszego czatu po CLI, nakładki
i etykiety Starting — poprawione. Końcowy odbiór PNG przez Opusa zaakceptował
układ po poprawkach szerokości kart, priorytetu tytułu, osi wyrównania, tła
nakładki i niedostępnych akcji Files. Drobny odstęp w skracanej nazwie gałęzi
usunięto następnie przez natywne środkowe wielokropki i ponownie obejrzano.

Ograniczenia: natywny odbiór dotyczył ciemnego motywu na Linuksie i danych
syntetycznych. Nie kwalifikuje jasnego motywu, macOS/Windows, wszystkich stanów
hover, IME, czytnika ekranu ani rzeczywistych procesów i ich cleanup. Nie
uruchamiano E2E ani release. Testy modelu nie zastępują tych kontroli.

# Aktualizacja GPUI Kit — 2026-10-08

GPUI Kit / Component / Base / Assets: 0.7.1, GPUI (`gpui-pre`) i klient
HTTP: 0.3.8. Rust pozostaje przypięty do 1.95.0. Aktualizacja pochodzi
z crates.io; [informacje o wydaniu](https://gpui-kit.com/releases/).

Root automatycznie montuje warstwy komponentów; usunięto ręczne renderowanie
warstw z okien aplikacji. Markdown używa `with_heading` z zachowaniem
dotychczasowych rozmiarów nagłówków. Notch zachowuje przezroczyste tło,
a obramowanie Root wynika z natywnych dekoracji okna. `frame-profile` włącza
teraz `gpui-kit/profiler`, bez dodatkowej bezpośredniej zależności od GPUI.

Weryfikacja na macOS:

- `cargo fmt --all -- --check` — OK.
- `cargo clippy --locked --all-targets --all-features -- -D warnings` — OK,
  łącznie z `dev-inspector` i `frame-profile`.
- `cargo test --locked --lib --bin canopy-desktop -- --skip ui::editor::tests --skip ui::session_inspector::tests --skip app_state::editors::tests`
  — 125 zaliczonych, 1 ignorowany, 6 odfiltrowanych testów interakcji GUI.
- `cargo tree --locked -i gpui-pre --depth 2` — jedna wersja GPUI: 0.3.8.

Nie uruchamiano aplikacji, E2E ani builda release. Wygląd, działanie warstw
w rzeczywistych oknach oraz platformy Windows/Linux wymagają osobnej weryfikacji.
Poniższe wyniki dotyczą wcześniejszego bootstrapu, nie tej aktualizacji.

# Weryfikacja bootstrapu — 2026-09-08

Środowisko: macOS, aarch64-apple-darwin, Rust/Cargo 1.95.0, profil dev.
Pakiet canopy-desktop 0.1.0; repozytorium main, bez początkowego commita.
Źródło zależności: crates.io. GPUI Kit / Component / Base / Assets 0.6.0,
GPUI (`gpui-pre`) 0.3.4. Domyślne features fasady: component, assets.
Opcjonalny feature aplikacji: dev-inspector. Bez gpui-fps.

## Wykonane kontrole

- `cargo fmt --all -- --check` — OK.
- `cargo build --locked` — OK, binarium macOS.
- `cargo clippy --locked --all-targets -- -D warnings` — OK.
- `cargo check --locked --features dev-inspector` — OK.
- Audyt skilla `audit_project.py` — brak findings i errors.
  Ręcznie sprawdzone hotspots: detach utrzymuje subskrypcję zamknięcia przez
  czas życia aplikacji; encje tworzone wyłącznie przy otwieraniu okna.
- `cargo tree --locked -i gpui-pre --depth 2` — jedna wersja GPUI 0.3.4.
- Odczyt rozwiązanych źródeł: fasada/init/assets, Root i warstwy (window, cx),
  callback on_window_closed (App, WindowId), menu i akcja Quit.
- Uruchomienie binarium oraz bundle `Canopy Dev.app` — OK.
- Rzeczywisty zrzut przez computer-use: widoczny tytuł, Canopy i podtytuł;
  zapis lokalny `target/canopy-window.png` (artefakt ignorowany przez Git).
- Cmd+Q — proces zakończony, potwierdzone przez pgrep.
- Ponowny start i natywny przycisk zamknięcia — proces zakończony.

## Przegląd według checklisty skilla

| Obszar | Wynik / dowód |
| --- | --- |
| Wersje, API | Tak: lockfile, źródła, build i Clippy |
| Stan, render, dane | Tak: statyczna encja, bez I/O i pętli odrysowań |
| Tożsamość, async, kolejki | Nie dotyczy: brak rekordów i usług |
| Layout | Tak w początkowym rozmiarze; minimum zadeklarowane, bez ręcznego testu resize |
| Motyw | Tokeny semantyczne; obejrzany jasny motyw. Ciemny i zmiana bez restartu niezweryfikowane |
| Wejście | Cmd+Q sprawdzony; brak pól tekstowych, IME i schowek nie dotyczy |
| Dostępność | Nie: brak pełnego testu czytnikiem ekranu |
| Błędy | Obsługa błędu tworzenia okna; brak usług/loading/offline |
| Okna | Zamknięcie i ponowny start sprawdzone; brak obsługi wielu okien |
| Pamięć, pomiary | Nie: brak profilowania pamięci i FPS; brak deklaracji 120 FPS |
| Regresje | Nie dotyczy: nowy bootstrap bez logiki domenowej; wykonany smoke test GUI |
| Wydanie | Nie: tylko lokalny bundle; brak podpisu/DMG, testów release, Windows i Linux |
| Dowody | Tak: rozdzielone kontrole wykonane i niewykonane |

Inspektor sprawdzony kompilacyjnie; interakcja z jego panelem niezweryfikowana.
