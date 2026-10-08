# Główne okno — pierwszy mock

Zakres: pierwsza iteracja ref-images/01-main-screen.png i 02-main-screen-with-git.png.
Nie jest jeszcze ukończonym odwzorowaniem 1:1 wszystkich referencji.

## Struktura

- `src/ui/theme.rs`: tokeny Canopy; neutralne kolory przeliczone do sRGB,
  alpha zachowana, typografia systemowa + wbudowany JetBrains Mono.
- `components.rs`: wiersze, sekcje, ikony, przyciski ikonowe, badge.
- `sidebar.rs`: projekty, pliki, narzędzia, wersja.
- `terminal.rs`: statyczne mocki Codex i Shell.
- `mod.rs`: stan tabów, inspector, resizable layout, status bar.
- `preferences.rs`: osobne okno Preferences z początkowym szkicem General.
- `assets.rs`: zasoby osadzone w binarium i fallback do GPUI Kit.

Pierwsze okno: 1423 × 892 jednostki logiczne. Titlebar 40, taby 32,
status bar 24, sidebar 220, inspector 280. Referencje PNG mają 2846 × 1784;
nie utożsamiamy fizycznych pikseli obrazu z jednostkami layoutu.

## Interakcje

- Kliknięcie tabów Shell / Codex oraz Cmd+1 / Cmd+2.
- Session / Changes; Cmd+Shift+S / Cmd+Shift+G.
- Oko / Cmd+B: ukrycie i ponowne pokazanie panelu.
- Przeciągane separatory; biblioteka utrzymuje rozmiary przez keyed state.
- Zębatka otwiera osobne Preferences (920 × 680, minimum 720 × 500).
  Ponowne kliknięcie aktywuje istniejące okno. Zamknięte można otworzyć ponownie.

## Weryfikacja 2026-09-08

- cargo fmt, cargo build --locked, Clippy --all-targets -D warnings: OK.
- cargo check --locked --features dev-inspector: OK przed końcową zmianą
  hostowania dialogu i przeniesieniem Preferences (zmiany nie dotyczą feature inspektora).
- GUI: obejrzane Session, Changes i Shell, sprawdzony Cmd+B i kliknięcie oka,
  przeciąganie lewego separatora, otwarcie Settings i Escape w pierwotnym wariancie modalnym; po zmianie
  na osobne okno sprawdzono otwarcie Preferences.
- Poprawiono ujawniony w GUI błąd: layer dialogu jako dziecko flex-column
  trafiał poza okno; teraz hostowany jest absolutnie w granicach okna.
- Zrzuty lokalne w target/mock-session.png i target/mock-changes.png.
- Brak pomiarów FPS, testów czytnika ekranu i innych platform.

## Pozostałe różnice / kolejna iteracja

- Mock danych, bez PTY, agenta, odczytu Git, edycji ani zapisu ustawień.
- Pliki i narzędzia sidebara, filtry Changes oraz pola Settings są prezentacyjne.
- Pełne Preferences, command bar i notch pozostają do wykonania.
- Pozostaje dopracowanie wysokości bloków terminala, scrollbarów, stanów hover/focus,
  znaków Git i plików, dokładnych kolorów akcentu oraz metryk fontów.
- Dotychczas porównano wizualnie; nie wykonano ilościowego pixel diff.

## Preferences — przeniesienie stylów Electrona

Przebudowano General według `PrefsHeader.svelte`, `PrefsSidebar.svelte`,
`PrefsSection.svelte`, `PrefsRow.svelte`, `GeneralPrefs.svelte` i tokens.css.
Osobne okno ma 920 × 720: 680 jednostek zawartości jak oryginalny modal
oraz 40 jednostek własnego paska tytułu. Minimum 720 × 540.

- Sidebar 208, header 48, padding treści 28/20, odstęp grup 28,
  odstęp sekcji od wierszy 12, pionowy padding wierszy 12, gap kolumn 24.
- Etykiety 13, pomoc 11, nagłówki sekcji 10, tytuł General 14;
  line-height odpowiednio 1.35 / 1.15 zgodnie ze źródłami.
- Kolory motywu przeliczane z oryginalnych OKLCH z niezaokrąglaną alpha.
  Border 0.12 odróżniony od border-subtle 0.06; input black/0.3,
  aktywna nawigacja white/0.08. Osobne okno ma nieprzezroczystą powierzchnię.
- GPUI rem ustawiony na 16 jak w CSS Electrona; tekst widoków ma jawne rozmiary.
  Synchronizowany również token primary kontrolki Checkbox.
- Dodano źródłowe ikony Lucide, przewijane obszary, search input,
  checkboxy i selektory z trwałym stanem encji.
- Build, Clippy -D warnings, format oraz dev-inspector check: OK.
- GUI: sprawdzono zmianę checkboxa, otwarcie selecta i widok General.
  Zrzut: target/preferences-general.png.

To nadal mock General: nawigacja innych kategorii, filtrowanie wyszukiwarką,
backup i uruchomienie wizarda nie są podłączone. Zmiany checkboxów i selectów
żyją tylko w oknie; nie modyfikują ustawień aplikacji Electron ani dysku.
Nie deklarujemy pixel-perfect zgodności renderowania fontów i całego menu GPUI.
