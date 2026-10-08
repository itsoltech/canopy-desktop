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
