# Przeniesienie desktopu do Rusta

Od 2026-10-08 rozwój desktopu odbywa się w `/Users/nix/GIT/canopy-desktop`
na branchu `rust-rewrite`.

## Pochodzenie

- Baza brancha: najnowszy pobrany `origin/next`,
  `d6e02e86223a0db204748ca82f8aaac76b249342`.
- Źródło Rusta: `/Users/nix/GIT/canopy-desktop-2`, commit
  `420cd7523bfc14b8ec4115de81a876e4f038e4cb`.
- Przeniesiono wszystkie śledzone pliki źródła Rusta: kod, testy, fixture,
  vendored Alacritty, zasoby, screeny, skrypty, toolchain, lockfile, dokumentację
  i skille projektu. Nie kopiowano `.git`, `target/` ani lokalnych cache.
- Historia Electrona pozostaje na `next` i w przodkach `rust-rewrite`.
  Referencję pliku można odczytać np. przez
  `git show next:src/main/taskTracker/providers/jira.ts`.

## Zachowane elementy

Cały `mobile/` jest identyczny z bazowym `next`, w tym jego lockfile i workflow
EAS. Zachowano też licencję, politykę bezpieczeństwa, changelog, CODEOWNERS,
formularze issues i ogólne workflow etykietowania, walidacji PR oraz auto-merge.
Dokumentacja `docs/features/remote-control.md` pozostaje referencją protokołu
Electrona potrzebną mobile.

Kod i konfigurację desktopu Electron/Svelte, jego testy, benchmarki, npm,
pakowanie i automatyzacje związane z tym stosem usunięto z brancha. CI desktopu
używa Cargo na macOS; workflow Windows pochodzi z repozytorium Rusta. Publikowanie
wydań Rusta pozostaje poza tą migracją. Nie uruchamiano workflow GitHub Actions.

## Granice

Rust nie ma jeszcze przeniesionego backendu Remote Control. Zachowanie mobile
w repozytorium nie oznacza możliwości połączenia go z desktopem Rust.
Polecenia desktopu w `mobile/README.md` dotyczą wersji Electron na `next`.

Dane aplikacji Rust pozostają w `~/Library/Application Support/Canopy Rust/`.
Migracja kodu nie zmienia baz, danych użytkownika, Keychain ani konfiguracji
mobile. Lokalne, ignorowane pliki wcześniejszego checkoutu są zachowane.
Repozytorium `canopy-desktop-2` pozostaje niezmienionym źródłem migracji.

## Praca lokalna

Desktop: `./scripts/run-macos.sh release` lub polecenia Cargo opisane w README.
Mobile: polecenia npm wykonywane wewnątrz `mobile/`.
Historyczne wyniki w `docs/verification.md` i innych dokumentach nie są nową
weryfikacją migracji ani kwalifikacją GUI/platform.

## Weryfikacja migracji — 2026-10-08

Sprawdzenia wykonano z katalogu `canopy-desktop` na macOS / Apple Silicon:

- `cargo fmt --all -- --check` — OK.
- `cargo clippy --locked --all-targets -- -D warnings` — OK.
- `cargo test --locked --lib` — 80 testów przeszło, 0 błędów, 1 pominięty
  test rzeczywistego połączenia SSH wymagający osobnego wywołania.
- Porównanie obiektów Git potwierdza identyczność kodu, testów, zasobów,
  natywnych adapterów, skryptów, vendora, skilli, toolchainu i plików Cargo
  ze źródłem. Obiekt całego `mobile/` jest identyczny z bazowym `next`.
- Kontrola whitespace kodu i zmienianej dokumentacji — OK. Pełne
  `git diff --cached --check` zgłasza wyłącznie istniejące w źródle białe znaki
  w importowanych referencjach skilla GPUI (`.agents/`, `.claude/`, `.pi/`);
  zachowano je bez zmian.

Do sprawdzeń użyto istniejącego cache przez
`CARGO_TARGET_DIR=/Users/nix/GIT/canopy-desktop-2/target`. To lokalne ustawienie
poleceń, nie zależność nowego repozytorium. Nie uruchamiano testów integracyjnych,
GUI/E2E, testów mobile ani workflow GitHub Actions w ramach migracji.
