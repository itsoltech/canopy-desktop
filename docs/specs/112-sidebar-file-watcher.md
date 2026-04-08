# SPEC: Dynamic sidebar file list via filesystem watcher

> Issue: [#112](https://github.com/itsoltech/canopy-desktop/issues/112) — bug: sidebar file list doesn't refresh after AI agent file changes
> Branch: `fix/sidebar-file-watcher`

## Wymagania

1. **Cel:** Sidebar file list ma odświeżać się dynamicznie, gdy agent AI (lub dowolny proces) tworzy/modyfikuje/usuwa pliki w workspace.
2. **Bez polling** — zakazane pełne skanowanie co X sekund. Kluczowa wydajność przy dużych projektach.
3. **Natywne FS events:**
   - macOS: FSEvents
   - Linux: inotify
   - Windows: ReadDirectoryChangesW
   - Biblioteka: **`@parcel/watcher`** (dodać do dependencies)
4. **Watcher lifecycle:** jeden watcher per-worktree, startuje przy `selectWorktree()`, gnie przy `detachProject()`
5. **Scope:** cały workspace rekurencyjnie
6. **Format eventów:** granularne `add`/`change`/`unlink` wysyłane do renderera przez IPC push
7. **Debouncing:** coalescing w oknie 50ms (burst npm install itp.)
8. **Ignore list:**
   - Hardcoded defaults (node_modules, .git, dist, build, .next, target, .venv, **pycache**, .DS_Store)
   - Globalna lista **edytowalna w Settings (Cmd+,)** przez UI z add/remove
   - Format: **glob patterns** (`node_modules`, `dist/**`, `**/*.log`)
   - Dotfiles (`.env`, `.gitignore`, `.prettierrc`, `.env.example`) są **widoczne** domyślnie
9. **Symlinki:** nie podążaj (`followSymlinks: false`)
10. **Rename:** natywnie wysyłane jako `unlink` + `add` (bez heurystyk)
11. **Otwarty plik usunięty:** `EditorPane` pokazuje banner "File deleted from disk" gdy watcher zgłosi `unlink` dla aktywnej ścieżki

## Code Style & Konwencje projektu

**Źródła:** `CLAUDE.md`, `.prettierrc.yaml` (single quote, no semi, 100 char), `eslint.config.mjs`, `tsconfig.web.json`, istniejące wzorce w `src/main/git/`.

**Kluczowe konwencje dla tego zadania:**

- **Error handling:** `neverthrow` (`Result<T, E>`, `ResultAsync<T, E>`) w main process. Dedicated `errors.ts` z typowanym union `_tag`. **Nie używać** try/catch w logice biznesowej. `fromExternalCall()` do wrappowania external promise. `unwrapOrThrow()` w IPC handlers.
- **Pattern matching:** `ts-pattern` (`match`/`with`/`.exhaustive()`) w error message formatterach i miejscach branchujących po `_tag`.
- **TypeScript:** `verbatimModuleSyntax: true` → type-only imports przez `import type`. Renderer bez strict mode.
- **Importy:** kolejność wg istniejącego wzorca (external → internal → types).
- **Nazewnictwo:** PascalCase dla klas (`FileTreeWatcher`), camelCase dla funkcji i zmiennych, SCREAMING_SNAKE_CASE dla consts (`DEFAULT_IGNORE_PATTERNS`).
- **IPC naming:** `domain:action` (np. `files:watch`, `files:unwatch`, `files:changed`).
- **Preload pattern:** każda metoda w `api` object, subscribe-style events zwracają unsubscribe function.
- **Renderer Svelte 5:** `$state`/`$derived`/`$effect`, `SvelteMap`/`SvelteSet` zamiast natywnych.

## Analiza złożoności

- **Scope:** średni/duży
- **Szacowana liczba plików:** ~12
- **Główne obszary:**
  - Main: nowa domena `src/main/fileWatcher/` (FileTreeWatcher, errors, defaults)
  - Main: `ipc/handlers.ts` (nowe handlery + integracja z WindowManager)
  - Main: `window/WindowManager.ts` (tracking watcherów per-window, analogicznie do `gitWatchers`)
  - Preload: `src/preload/index.ts` + `index.d.ts` (nowe API)
  - Renderer store: `src/renderer/src/lib/stores/fileTree.svelte.ts` (handlery push events)
  - Renderer component: `src/renderer/src/components/sidebar/FileTreeSection.svelte` (subskrybcja)
  - Renderer component: `src/renderer/src/components/editor/EditorPane.svelte` (banner)
  - Preferences: `src/renderer/src/lib/stores/preferences.svelte.ts` (default dla `files.ignorePatterns`)
  - Preferences UI: nowy `FileWatcherPrefs.svelte` + integracja z `PreferencesModal`
  - `package.json` (nowa dependency)
- **Zależności między fazami:** F1 → F2 → F3 → F4. F5/F6 mogą iść równolegle po F4.

---

## Plan implementacji

### Faza 1: Backend watcher — klasa + errors + defaults

**Cel:** Utworzyć `FileTreeWatcher` analogiczny do `GitWatcher`, z typowanymi błędami neverthrow, defaultową listą ignorów i coalescing 50ms.

**Lokalizacje:**

- `package.json` — dodać `@parcel/watcher`
- `src/main/fileWatcher/errors.ts` — nowy plik, typed errors
- `src/main/fileWatcher/defaults.ts` — nowy plik, `DEFAULT_IGNORE_PATTERNS`
- `src/main/fileWatcher/FileTreeWatcher.ts` — nowy plik, główna klasa
- Wzorzec odniesienia: `src/main/git/GitWatcher.ts`, `src/main/git/errors.ts`

**Zadania:**

- [ ] `npm install @parcel/watcher`
- [ ] Utwórz `errors.ts`:

  ```ts
  import { match } from 'ts-pattern'

  export type FileWatcherError =
    | { _tag: 'WatchStartFailed'; path: string; message: string }
    | { _tag: 'WatchStopFailed'; path: string; message: string }
    | { _tag: 'SubscribeFailed'; path: string; message: string }

  export function fileWatcherErrorMessage(error: FileWatcherError): string {
    return match(error)
      .with(
        { _tag: 'WatchStartFailed' },
        (e) => `Failed to start watcher at ${e.path}: ${e.message}`,
      )
      .with({ _tag: 'WatchStopFailed' }, (e) => `Failed to stop watcher at ${e.path}: ${e.message}`)
      .with({ _tag: 'SubscribeFailed' }, (e) => `Failed to subscribe to ${e.path}: ${e.message}`)
      .exhaustive()
  }
  ```

- [ ] Utwórz `defaults.ts` z eksportem `DEFAULT_IGNORE_PATTERNS` (lista: `node_modules`, `.git`, `dist`, `build`, `.next`, `target`, `.venv`, `__pycache__`, `.DS_Store`, `coverage`, `.turbo`, `.cache`).
- [ ] Utwórz `FileTreeWatcher` class:
  - Konstruktor: `(repoRoot: string, ignorePatterns: string[], onChange: (events: FileChangeEvent[]) => void)`
  - Typ `FileChangeEvent = { type: 'add' | 'change' | 'unlink'; path: string }` (path relatywny do `repoRoot`)
  - `start(): ResultAsync<void, FileWatcherError>` — `@parcel/watcher` subscribe, opcje: `{ ignore: [...ignorePatterns] }`
  - `stop(): ResultAsync<void, FileWatcherError>` — `subscription.unsubscribe()`
  - Debounce buffer 50ms: kolekcjonuj eventy w `pendingEvents: FileChangeEvent[]`, po 50ms wywołuj `onChange(events)` i resetuj
  - Deduplikacja: jeśli to samo `path+type` wystąpi kilka razy w buforze, trzymać tylko ostatnie
  - Metoda `updateIgnorePatterns(patterns: string[])` — restart watchera z nową listą (na wypadek zmiany w Settings)

**Wzorce do zastosowania:**

```ts
// Schemat start() — wzoruj się na GitWatcher.start() + integracja z neverthrow
import subscribe from '@parcel/watcher'
import { ResultAsync } from 'neverthrow'
import { fromExternalCall, errorMessage } from '../errors'

start(): ResultAsync<void, FileWatcherError> {
  if (this.subscription) return okAsync(undefined)
  return fromExternalCall(
    subscribe(this.repoRoot, this.handleEvents, { ignore: this.ignorePatterns }),
    (e) => ({ _tag: 'WatchStartFailed' as const, path: this.repoRoot, message: errorMessage(e) })
  ).map((sub) => {
    this.subscription = sub
  })
}
```

**Tech notes:**

- **Referencja:** struktura klasy i debounce logic analogiczna do `src/main/git/GitWatcher.ts` (timer `ReturnType<typeof setTimeout> | null`, `scheduleRefresh`).
- **Różnica od GitWatcher:** chokidar używa event listenerów, `@parcel/watcher` ma jedną funkcję `subscribe(path, callback, options)` zwracającą obiekt z `unsubscribe()`.
- **Gotcha:** @parcel/watcher wysyła absolute paths — w handlerze przekonwertuj na relative do `repoRoot` przed wysłaniem do renderera (mniej dataflow, łatwiej mapować do drzewa w UI).
- **Gotcha:** subscribe zwraca `Promise<Subscription>` — musi być `await` przed zarejestrowaniem. Pamiętaj o race condition przy szybkim start/stop.
- **Edge case:** worktree może być usunięty podczas działania — `@parcel/watcher` rzuci błąd w callbacku, złap i rozgłoś `WatchStartFailed` (main tylko loguje, nie crashuje).
- **Walidacja fazy:** `npm run typecheck:node` przechodzi. Klasa da się zaimportować bez błędów.

---

### Faza 2: IPC handlers + lifecycle w WindowManager

**Cel:** Wystawić `files:watch` / `files:unwatch` IPC, trackować watchery per-window analogicznie do `gitWatchers`.

**Zależy od:** Faza 1

**Lokalizacje:**

- `src/main/window/WindowManager.ts` — nowa mapa `fileWatchers: Map<number, FileTreeWatcher>`
- `src/main/ipc/handlers.ts` — nowe handlery + integracja z existing `app:detachProject`

**Zadania:**

- [ ] W `WindowManager` dodaj:
  - `private fileWatchers = new Map<number, FileTreeWatcher>()` (key: `windowId`)
  - `setFileWatcher(windowId, watcher)`, `disposeFileWatcher(windowId)` (wzór: istniejące `setGitWatcher`, `disposeGitWatcher`)
  - `disposeFileWatcher` czyści mapę i wywołuje `stop()` ignorując errors
- [ ] W `handlers.ts` dodaj:

  ```ts
  ipcMain.handle('files:watch', async (event, payload: { repoRoot: string }) => {
    const window = BrowserWindow.fromWebContents(event.sender)
    if (!window) return
    const windowId = window.id

    // Zdemontuj poprzedni watcher dla tego window (worktree switch)
    windowManager.disposeFileWatcher(windowId)

    const patterns = await getIgnorePatternsFromPrefs()
    const watcher = new FileTreeWatcher(payload.repoRoot, patterns, (events) => {
      event.sender.send('files:changed', { repoRoot: payload.repoRoot, events })
    })
    const result = await watcher.start()
    if (result.isErr()) throw new Error(fileWatcherErrorMessage(result.error))
    windowManager.setFileWatcher(windowId, watcher)
  })

  ipcMain.handle('files:unwatch', async (event) => {
    const window = BrowserWindow.fromWebContents(event.sender)
    if (window) windowManager.disposeFileWatcher(window.id)
  })
  ```

- [ ] `getIgnorePatternsFromPrefs()`: odczyt z `PreferencesStore` klucza `files.ignorePatterns` (JSON array). Jeśli brak — zwróć `DEFAULT_IGNORE_PATTERNS`.
- [ ] Dodaj handler `files:updateIgnorePatterns` wywoływany gdy user edytuje listę w Settings — iteruje po `fileWatchers` i wywołuje `updateIgnorePatterns()` dla każdego.
- [ ] Window `closed` hook — wywołaj `disposeFileWatcher(windowId)` dla usuwanego window.

**Wzorce do zastosowania:**

```ts
// Schemat trackingu - identyczny do gitWatchers w WindowManager
// Odszukaj istniejący setGitWatcher/disposeGitWatcher i zreplikuj strukturę 1:1
```

**Tech notes:**

- **Integracja:** watcher uruchamia się w `files:watch` handler wywoływanym z rendera w `workspace.svelte.ts`. NIE automatycznie przy `app:setActiveWorktree` — renderer decyduje kiedy startować.
- **Gotcha:** przy szybkim przełączaniu worktree (user klika różne) — `disposeFileWatcher` przed `setFileWatcher` zapobiega wyciekom.
- **Performance:** `disposeFileWatcher` nie może być blocking — wrzuć stop w `.catch(() => {})` i kontynuuj.
- **Walidacja:** ręcznie sprawdź w dev: otwórz workspace, utwórz plik `touch test.md` w terminalu, obserwuj devtools Main process console — powinien widzieć event.

---

### Faza 3: Preload bridge — expose API

**Cel:** Wystawić `watchFiles`, `unwatchFiles`, `onFilesChanged` w `window.api`.

**Zależy od:** Faza 2

**Lokalizacje:**

- `src/preload/index.ts` — nowe metody w `api` object
- `src/preload/index.d.ts` — typy

**Zadania:**

- [ ] Dodaj do `api`:
  ```ts
  watchFiles: (repoRoot: string) => ipcRenderer.invoke('files:watch', { repoRoot }),
  unwatchFiles: () => ipcRenderer.invoke('files:unwatch'),
  onFilesChanged: (callback: (payload: { repoRoot: string; events: FileChangeEvent[] }) => void) => {
    const handler = (_event, payload) => callback(payload)
    ipcRenderer.on('files:changed', handler)
    return () => ipcRenderer.removeListener('files:changed', handler)
  },
  updateIgnorePatterns: (patterns: string[]) => ipcRenderer.invoke('files:updateIgnorePatterns', { patterns }),
  ```
- [ ] Uzupełnij `index.d.ts` z odpowiednimi typami (zaimportuj `FileChangeEvent` lub zduplikuj strukturę).

**Wzorce do zastosowania:**

```ts
// Subscribe-style: wzoruj się na istniejące onGitChanged (linia ~414)
// Return unsubscribe function, nie callback reference
```

**Tech notes:**

- **Convention:** subscribe API zwraca unsubscribe function. Renderer zapisuje `const off = window.api.onFilesChanged(...)` i wywołuje `off()` na cleanup.
- **Edge case:** typy w `index.d.ts` muszą być eksportowalne — `FileChangeEvent` może wymagać współdzielonego typu (utwórz `src/shared/types/files.ts` jeśli już nie ma konwencji współdzielenia).
- **Walidacja:** `npm run typecheck` przechodzi (node + svelte).

---

### Faza 4: Renderer — store integration + subscribe push events

**Cel:** Rozszerzyć `fileTree` store o obsługę push events (add/change/unlink). Start/stop watchera przy `attachProject`/`detachProject`.

**Zależy od:** Faza 3

**Lokalizacje:**

- `src/renderer/src/lib/stores/fileTree.svelte.ts` — metoda `applyFileEvents(events)`
- `src/renderer/src/components/sidebar/FileTreeSection.svelte` — subskrybcja push events w $effect
- `src/renderer/src/lib/stores/workspace.svelte.ts` — wywołanie `watchFiles` / `unwatchFiles` w `attachProject` / `detachProject`

**Zadania:**

- [ ] W `fileTree.svelte.ts` dodaj `applyFileEvents(events: FileChangeEvent[])`:
  - Iteruje po events i selektywnie odświeża tylko dotknięte foldery przez istniejące `expandDir()`
  - **Strategia selektywna:** dla każdego eventu wylicz `parentDir`, zbierz unikalne `parentDirs` w `SvelteSet`, wywołaj `expandDir()` tylko dla tych którzy są w `expandedDirs`
  - Jeśli event dotyczy pliku w folderze który nie jest rozwinięty — ignoruj
- [ ] W `workspace.svelte.ts`:
  - `attachProjectImpl` (linia ~267 gdzie jest `gitWatch`) — dodaj `void window.api.watchFiles(info.repoRoot)`
  - `detachProject` (linia ~340 gdzie jest `gitUnwatch`) — dodaj `void window.api.unwatchFiles()`
- [ ] W `FileTreeSection.svelte` dodaj subskrybcję push events:
  - W istniejącym `$effect`, po subskrypcji `onGitChanged`, dodaj:
    ```ts
    const offFiles = window.api.onFilesChanged(({ repoRoot, events }) => {
      if (repoRoot !== currentWorktree) return
      fileTree.applyFileEvents(events)
    })
    return () => {
      offGit()
      offFiles()
    }
    ```

**Wzorce do zastosowania:**

```ts
// Schemat applyFileEvents — wzoruje na refreshGitStatus (jest już w fileTree.svelte.ts)
function applyFileEvents(events: FileChangeEvent[]): void {
  const root = rootPath
  if (!root) return

  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const dirsToRefresh = new Set<string>()
  for (const ev of events) {
    const absPath = `${root}/${ev.path}`
    const parent = absPath.substring(0, absPath.lastIndexOf('/'))
    if (expandedDirs[parent]) {
      dirsToRefresh.add(parent)
    }
  }

  if (dirsToRefresh.size === 0) return
  void Promise.all([...dirsToRefresh].map((dir) => expandDir(dir)))
}
```

**Tech notes:**

- **Referencja:** `refreshGitStatus` w `fileTree.svelte.ts` (linie 51-106) pokazuje wzorzec: zbieraj unikalne foldery, `Promise.all` dla `expandDir`.
- **Gotcha:** `@parcel/watcher` wysyła paths relatywne po konwersji z Fazy 1 — UPEWNIJ SIĘ że w renderze używasz `rootPath` + relative path do rekonstrukcji absolute path. Ścieżki w `expandedDirs` to absolute.
- **Gotcha:** nie wywołuj `expandDir` synchronicznie dla wielu folderów naraz — Svelte zareactuje na każdą zmianę. Zamiast tego `Promise.all`.
- **Edge case:** event może dotyczyć pliku w folderze który nie istnieje w `expandedDirs` — to OK, ignorujemy (folder nie jest widoczny w UI).
- **Edge case:** worktree switch w trakcie burst eventów — `FileTreeSection` sprawdza `payload.repoRoot === currentWorktree` i ignoruje stale events.
- **Performance:** dedup przez `SvelteSet`, batch refresh przez `Promise.all`.
- **Walidacja:** po zmianie — utwórz plik w terminalu, sprawdź że pojawia się w sidebar w < 200ms. Usuń plik — znika.

---

### Faza 5: Settings UI — edytowalna lista ignore patterns

**Cel:** Użytkownik może w Preferences (Cmd+,) edytować listę globalnych ignorów przez UI z add/remove.

**Zależy od:** Faza 4

**Lokalizacje:**

- `src/renderer/src/components/preferences/FileWatcherPrefs.svelte` — **nowy plik**
- `src/renderer/src/components/preferences/PreferencesModal.svelte` — dodaj sekcję
- `src/renderer/src/lib/stores/preferences.svelte.ts` — default value dla `files.ignorePatterns`

**Zadania:**

- [ ] W `preferences.svelte.ts` dodaj default dla `files.ignorePatterns` (JSON stringified array). Źródłem prawdy dla listy jest `src/main/fileWatcher/defaults.ts` — albo zduplikuj, albo przenieś do `src/shared/` i zaimportuj w obu miejscach.
- [ ] `FileWatcherPrefs.svelte` komponent:
  - Stan: `patterns: string[] = $derived(JSON.parse(prefs['files.ignorePatterns'] || '[]'))`
  - UI: lista chipów/tagów z przyciskiem X do usunięcia każdego (wzór: `SidebarPrefs.svelte`)
  - Input + przycisk "Add" do dodania nowego wzorca
  - Przycisk "Reset to defaults"
  - Po każdej zmianie: `setPref('files.ignorePatterns', JSON.stringify(next))` + `window.api.updateIgnorePatterns(next)`
  - Komunikat pomocniczy: "Patterns use glob syntax. Examples: `node_modules`, `dist/**`, `**/*.log`"
- [ ] `PreferencesModal.svelte`: dodaj nową podkategorię "File Watcher" w sekcji "Dev Tools".

**Wzorce do zastosowania:**

```svelte
<!-- Wzoruj na SidebarPrefs.svelte dla layoutu sekcji z listą edytowalnych wpisów -->
<section class="pref-section">
  <h3>Ignored paths</h3>
  <p class="pref-description">Patterns to exclude from the file watcher...</p>
  <!-- lista chipów + input + add button -->
</section>
```

**Tech notes:**

- **Referencja:** `src/renderer/src/components/preferences/SidebarPrefs.svelte` pokazuje wzorzec listy konfigurowalnych wpisów. `TerminalPrefs.svelte` dla typowego layoutu sekcji.
- **Gotcha:** po każdej edycji WYŚLIJ update do main process aby aktywne watchery zrestartowały — inaczej user musi zamykać/otwierać workspace.
- **Gotcha:** walidacja input — nie dopuszczaj pustych stringów, trim whitespace. Duplikaty filtruj.
- **UX:** dodaj tooltip/help icon wyjaśniający że te wzorce stosują się tylko do file watchera (nie do search, nie do git).
- **Walidacja:** otwórz Settings, dodaj wzorzec `tmp/**`, utwórz `tmp/x.txt` w terminalu — nie pojawia się w sidebar. Usuń wzorzec — pojawia się.

---

### Faza 6: EditorPane banner + finalny verify

**Cel:** Gdy otwarty plik zostanie usunięty z dysku, panel pokazuje banner. Cała zmiana przechodzi lint/typecheck/manual test.

**Zależy od:** Faza 4

**Lokalizacje:**

- `src/renderer/src/components/editor/EditorPane.svelte`

**Zadania:**

- [ ] Dodaj stan `fileDeleted = $state(false)`
- [ ] Subskrypcja `onFilesChanged` w `$effect`:
  ```ts
  const off = window.api.onFilesChanged(({ events }) => {
    for (const ev of events) {
      const absPath = /* reconstruct from repoRoot + ev.path */
      if (absPath !== filePath) continue
      if (ev.type === 'unlink') fileDeleted = true
      if (ev.type === 'add' || ev.type === 'change') {
        fileDeleted = false
        void loadFile()
      }
    }
  })
  return () => off()
  ```
- [ ] Banner w template (wzoruj na `ExitBanner.svelte`):
  ```svelte
  {#if fileDeleted}
    <div class="banner banner-warning">
      <span>File deleted from disk.</span>
    </div>
  {/if}
  ```
- [ ] Użyj istniejących CSS variables (`--c-warning-text`, `--c-warning-bg`) — zobacz inne banery w projekcie.
- [ ] `npm run lint`, `npm run typecheck`, ręczny test E2E.

**Wzorce do zastosowania:**

```svelte
<!-- Wzorze na ExitBanner.svelte (src/renderer/src/components/terminal/ExitBanner.svelte) -->
```

**Tech notes:**

- **Referencja:** `src/renderer/src/components/terminal/ExitBanner.svelte` ma wzorzec baneru z przyciskiem akcji.
- **Gotcha:** cleanup subscription — return z effect musi wywołać `off()` inaczej memory leak.
- **Edge case:** `add` event po `unlink` dla tej samej ścieżki (przenoszenie pliku) — `fileDeleted = false` i `loadFile()` przywraca zawartość.
- **Edge case:** plik poza workspace (np. `~/Downloads`) — watcher go nie obserwuje, banner się nie pokaże. To jest OK, scope watchera jest workspace-bound.
- **Gotcha:** `EditorPane` ma już error handling (linie 99-100). Banner to osobny stan, nie miesz z `error`.

---

## Weryfikacja końcowa

**Testy manualne (macOS priorytet, Linux/Windows jeśli dostępne):**

- [ ] Otwórz workspace, rozwiń folder w sidebar
- [ ] W terminalu: `touch new-file.md` — plik pojawia się bez refresh
- [ ] `rm new-file.md` — plik znika
- [ ] `mkdir newdir && touch newdir/a.txt` — folder się pojawia (po expand widoczny plik)
- [ ] Uruchom Claude Code w tab, niech utworzy plik — pojawia się dynamicznie w sidebar
- [ ] Otwórz plik w `EditorPane`, usuń z terminala — banner się pokazuje
- [ ] Otwórz Settings → Dev Tools → File Watcher → dodaj `tmp/**`, utwórz `tmp/x.txt` — NIE pojawia się
- [ ] Usuń wzorzec `tmp/**` — `tmp/x.txt` pojawia się po refresh folderu
- [ ] Przełącz worktree — watcher poprzedniego się gnie, nowy startuje
- [ ] Detach project — watcher stopuje
- [ ] `npm install` w terminalu — sidebar NIE zacina (ignoruje `node_modules`)
- [ ] Otwórz duży projekt (1000+ plików) — brak lagów

**Testy automatyczne:**

- [ ] `npm run lint` — 0 errors
- [ ] `npm run typecheck` — 0 errors
- [ ] `npm run build` — successful build

**Kryteria akceptacji:**

- [ ] Sidebar odświeża się automatycznie przy zmianach plików przez AI agenta
- [ ] Brak polling — `rg "setInterval" src/main/fileWatcher` nic nie zwraca
- [ ] Duże projekty (10k+ plików z `node_modules` ignorowanym) nie powodują wzrostu RAM/CPU powyżej baseline
- [ ] User może edytować listę ignorów w Settings i zmiany stosują się natychmiast
- [ ] `EditorPane` pokazuje banner gdy otwarty plik zostanie usunięty
- [ ] Kod zgodny z projektowymi konwencjami (neverthrow, ts-pattern, brak try/catch w logice biznesowej)
- [ ] PR utworzony i zmergowany do `next`
