# Canopy — zasady pracy

- Rozwijamy Rust + `gpui_kit` na `rust-rewrite`. Referencja produktu: Electron
  na `next` i `ref-images/`. Odtwarzaj wygląd 1:1 bez samowolnego redesignu;
  późniejsze decyzje użytkownika mają pierwszeństwo. `mobile/` jest osobnym projektem.
- Dostarczaj całe zachowanie: UI, stan, operacje, błędy, persist/restore i cleanup.
  Korzystaj z istniejących komponentów; nie przedstawiaj makiet jako integracji.
- Zachowuj niezwiązane zmiany, dane i sekrety użytkownika. Commit/push wykonuj
  tylko na zlecenie. Nie resetuj bazy ani nie zmieniaj globalnej konfiguracji CLI.
- Zachowuj przypięte zależności i Cargo.lock; Cargo uruchamiaj z `--locked`.
  API sprawdzaj w rozwiązanych źródłach. Git aplikacji korzysta z libgit2.
- Stan kontrolek i zadań ma jawnego właściciela. Render nie wykonuje I/O ani
  nie tworzy procesów. Tokeny UI pochodzą z `src/ui/theme.rs`.

## Testy i weryfikacja

Priorytetem jest kompletna funkcjonalność, nie liczba testów. Korzystaj z istniejących
testów; nowe dodawaj tylko dla istotnego zachowania lub konkretnej regresji.
Bez tautologii, testów przechowywania pól DTO i asercji kopiujących implementację.
Szersze uzupełnianie testów to osobny etap. Szczegóły: [docs/testing.md](docs/testing.md).

Uruchamiaj dozwolone, adekwatne sprawdzenia; naprawiaj błędy wynikające ze zmiany.
Nie powtarzaj pomyślnych kontroli bez powodu. E2E wymaga wcześniejszej zgody.
Raportuj wykonane sprawdzenia i ograniczenia; lint/testy nie potwierdzają działania GUI.

## Dokumentacja według zakresu zmiany

Czytaj tylko materiały dotyczące zadania. Uzgodnione reguły zachowania są w
[kontraktach projektu](docs/project-contracts.md) — wybierz odpowiednią sekcję,
nie wczytuj całego dokumentu do każdej poprawki. Opisy modułów i procedur:

- Architektura i workspace: [stan](docs/app-state.md), [persist/restore](docs/persistence.md).
- UI, layout, sidebar i okna: [komponenty](docs/components.md), [motion](docs/motion.md).
- Terminal, PTY i procesy: [terminal](docs/terminal.md), [narzędzia](docs/tools.md).
- Settings, profile i sekrety: [SQLite](docs/settings.md), [preferencje agentów](docs/agent-preferences.md).
- Sesje agentów i resume: [agenci](docs/agents.md), [notch](docs/notch.md), [toasty](docs/toasts.md).
- Git: [worktree](docs/git-worktrees.md), [Pull/Push](docs/git-network.md),
  [Changes i commit](docs/git-changes.md), [hooki](docs/git-hooks.md).
- Files, edytor i podglądy: [pliki](docs/files-editor.md).
- Tasks i konta: [integracje](docs/integrations.md), [Jira](docs/jira.md),
  [YouTrack](docs/youtrack.md), [filtry i załączniki](docs/task-browser.md),
  [task → worktree](docs/task-worktree.md).
- Platformy i wydawanie: [migracja Rust/mobile](docs/rust-rewrite.md),
  [Windows](docs/windows.md), [wydajność](docs/performance.md).
- Dobór sprawdzeń: [testy](docs/testing.md); dawne wyniki: [weryfikacja](docs/verification.md).

Aktualizuj dokumentację zmienianej funkcji. Stan implementacji i wersje sprawdzaj
w kodzie i Cargo.lock; historyczne raporty nie są dowodem obecnego działania.
`AGENTS.md` pozostaje krótkim przewodnikiem: szczegółowe reguły dopisuj w dokumentacji
tematycznej i tutaj dodawaj odnośnik tylko wtedy, gdy ułatwia jej znalezienie.
