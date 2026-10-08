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
