# Canopy Desktop — Rust + GPUI

Natywna aplikacja Canopy rozwijana w tym repozytorium na branchu `rust-rewrite`.
Kod Rust + GPUI Kit został przeniesiony z `canopy-desktop-2`; implementacja
Electron + Svelte pozostaje w historii brancha `next`.

`mobile/` zawiera dotychczasową, niezależną aplikację Expo/React Native.
Polecenia npm wykonujemy w `mobile/`; desktop korzysta z Cargo. Integracja
Remote Control z desktopem Rust nie została przeniesiona. Zakres i pochodzenie
kodu: [docs/rust-rewrite.md](docs/rust-rewrite.md).

## Środowisko

- Rust 1.95.0, rustfmt i Clippy — wersje w `rust-toolchain.toml`.
- GPUI Kit **0.6.0**, komponenty i wbudowane zasoby ikon.
- Pełny graf zależności przypięty w `Cargo.lock`; zachowujemy go w Git.
- Początkowa platforma weryfikacji: macOS / Apple Silicon.
- Xcode z narzędziami deweloperskimi i SDK macOS (`xcode-select -p`).

Po instalacji Rust przez rustup Cargo pobierze wskazany toolchain i zależności.
Pierwsza kompilacja GPUI jest znacznie dłuższa niż kolejne kompilacje aplikacji.

```sh
cargo dev
# To samo: cargo run --locked
cargo inspect
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked
```

`./scripts/run-macos.sh` domyślnie buduje i otwiera **zoptymalizowany release**:
`target/release/Canopy.app`. `./scripts/run-macos.sh dev` uruchamia build
bez optymalizacji; `cargo dev` również pozostaje poleceniem debug.
Pakiety są lokalne, bez podpisu wydawniczego.

`cargo inspect` włącza feature inspektora GPUI; skrót: Cmd+Option+I. Profil do przyszłych pomiarów:
`cargo build --locked --profile profiling`. Nie jest to potwierdzenie 120 FPS.

## Struktura

- `src/main.rs` — wejście procesu.
- `src/app.rs` — inicjalizacja GPUI, menu, cykl życia okna i początkowy widok.
- `docs/verification.md` — zakres sprawdzeń i ograniczenia.

Okno startuje w rozmiarze 1423 × 892 jednostek logicznych, z minimum
800 × 500. Używa semantycznego motywu GPUI Kit oraz `Root` z warstwami
sheet/dialog/notification. Cmd+Q i zamknięcie ostatniego okna kończą proces.

## Interfejs

Kod widoków i motywu: `src/ui/`. Terminal/PTY, projekty i worktree, layout,
persist/restore, pliki i edytor, Git Changes oraz sesje agentów mają integracje.
Część Preferences i metryki zasobów nadal zawierają mocki; szczegóły i granice
kwalifikacji opisują dokumenty poszczególnych modułów. [docs/ui-mock.md](docs/ui-mock.md)
jest historycznym opisem pierwszego etapu interfejsu.

## Stan aplikacji

Etap bazowego UI jest zamknięty. Warstwa ustawień ma typowany kontrakt,
asynchroniczny worker SQLite oraz import zgodnej bazy Electrona do osobnej
kopii. API, CLI i kolejność integracji: [docs/settings.md](docs/settings.md).
Kontrolki General korzystają już ze wspólnego stanu i SQLite. Taby, splity,
przenoszenie pane’ów i layout sidebarów mają modele domenowe. Architektura,
skróty i zakres implementacji: [docs/app-state.md](docs/app-state.md).
Open folder / ⌘O otwiera rzeczywisty katalog. Lista projektów i aktywny projekt
są odtwarzane z SQLite zgodnie z preferencją startup. Bez projektu aplikacja
pokazuje centralny CTA. Pełny layout tabów i pane’ów jest zapisywany i odtwarzany wraz z metadanymi.
Format i kontrakt lazy-start PTY: [docs/persistence.md](docs/persistence.md).

Nie ma jeszcze pakowania wydaniowego / DMG, podpisu wydawniczego ani aktualizacji.
Kwalifikacja platform jest opisana oddzielnie; nie oznacza pełnej weryfikacji GUI
na Windows/Linux. Repozytorium zachowuje dotychczasowy [LICENSE.md](LICENSE.md).

Implementacja i jawna macierz kwalifikacji Windows są prowadzone w
[docs/windows.md](docs/windows.md).

Źródło biblioteki: [GPUI Kit 0.6.0](https://gpui-kit.com/releases/).

## Profilowanie

```sh
./scripts/run-macos.sh release
./scripts/run-macos.sh profiling --features frame-profile
./scripts/run-macos.sh dev --features frame-profile
```

W wariancie `frame-profile` Ctrl+Option+P rozpoczyna 30-sekundowy zapis
`Window::draw` i kosztu przekazania klatki platformie. CSV trafia do
`$TMPDIR/canopy-profiles` (lub katalogu `CANOPY_PROFILE_DIR`). Dane grupujemy
według okna i próby: `python3 scripts/summarize-frames.py /path/frames-*.csv`.
Nie są to timestampy faktycznego wyświetlenia klatki przez monitor.
Release bez tego feature nie zawiera rejestratora ani jego wątku.

Wyniki profilowania i porównanie przed/po: [docs/performance.md](docs/performance.md).

API i przykłady wspólnych komponentów: [docs/components.md](docs/components.md).

Nakładka macOS i jej natywne pozycjonowanie: [docs/notch.md](docs/notch.md).

Wspólne tokeny i mechanizmy animacji: [docs/motion.md](docs/motion.md).


## Terminal

Pane'y terminalowe korzystają teraz z Alacritty Terminal i rzeczywistego PTY.
Powłoka/narzędzie startuje bezpośrednio jako program z argumentami i środowiskiem
użytkownika. Po zakończeniu pane pokazuje exit code oraz Restart / Close.
Restore uruchamia tylko aktywny tab. Szczegóły i sprawdzenia:
[docs/terminal.md](docs/terminal.md).
