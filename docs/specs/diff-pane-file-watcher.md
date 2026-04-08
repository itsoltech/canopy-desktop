# SPEC: Diff pane on FileTreeWatcher + GitWatcher migration

> Branch: `fix/sidebar-file-watcher` (continuation — same PR #114)

## Wymagania

1. **Cel:** `DiffPane` i `ChangesPanel` mają reagować na **realne zmiany plików**, nie tylko na zmiany git metadata. Obecnie chybiają nowe pliki utworzone przez agentów — sidebar widzi je od razu, diff nie.
2. **Zachowanie jak GitHub diff:** pokazuj wszystko co `git diff HEAD` + `git ls-files --others --exclude-standard` zwracają. Użytkownicze `files.ignorePatterns` z Settings NIE mają wpływać na diff — tylko `.gitignore`.
3. **Jeden watcher na worktree:** wspólny event stream między sidebar i diff. Filtrowanie per-konsument po stronie rendera.
4. **Safety ignores w watcher:** hardcoded `node_modules`, `.git`, `dist`, `build`, `.next`, `target`, itd. — żeby nie zabić wydajności przy `npm install`. User ignore patterns (z prefs) aplikowane TYLKO w rendererze dla sidebar.
5. **Migracja GitWatcher na `@parcel/watcher`:** wywal `chokidar` z deps. Jeden stack do FS watching.
6. **Debounce diff refresh:** 200ms — bufor na burst'y przy branch switch / npm install.
7. **ChangesPanel = DiffPane:** ten sam fix dla komponentu sidebara zmian.
8. **Zachowaj `onGitChanged` subskrypcję:** dla branch/worktree switch i metadata events. Ale dodajemy drugą subskrypcję (`onFilesChanged`) dla file events.

## Code Style & Konwencje projektu

**Źródła:** `CLAUDE.md`, `src/main/fileWatcher/*` (już istniejące wzorce), `src/main/git/GitWatcher.ts`.

**Kluczowe konwencje dla tego zadania:**

- **neverthrow** dla error handling w main, **ts-pattern** dla error formatting
- `verbatimModuleSyntax: true` → `import type` dla typów
- Svelte 5 `$state`/`$effect`/`onMount` patterns
- IPC naming: kontynuuj `files:*` prefix
- `SvelteSet` zamiast natywnego `Set` w renderze (lint rule)
- Comment style jak w istniejących klasach (jsdoc dla public API)

## Analiza złożoności

- **Scope:** średni/duży
- **Szacowana liczba plików:** ~10
- **Główne obszary:**
  - Main: `src/main/fileWatcher/FileTreeWatcher.ts` (refactor)
  - Main: `src/main/fileWatcher/defaults.ts` (rozdzielić safety vs user defaults)
  - Main: `src/main/git/GitWatcher.ts` (migracja z chokidar na @parcel/watcher)
  - Main: `src/main/ipc/handlers.ts` (simplify `files:watch` + `files:updateIgnorePatterns`)
  - Renderer: `src/renderer/src/lib/stores/fileTree.svelte.ts` (filter w `applyFileEvents`)
  - Renderer: `src/renderer/src/components/diff/DiffPane.svelte` (add `onFilesChanged` + debounce)
  - Renderer: `src/renderer/src/components/diff/ChangesPanel.svelte` (to samo)
  - `package.json` (usuń `chokidar`)
- **Zależności faz:** F1 → F2 → F3 równolegle z F4 → F5/F6 → F7

---

## Plan implementacji

### Faza 1: Refactor `FileTreeWatcher` — emituj wszystko, safety ignore

**Cel:** Watcher przestaje filtrować user ignore patterns natywnie. Dostaje hardcoded `SAFETY_IGNORE_PATTERNS` (generated dirs które zawsze mają być ignorowane — niezależnie od ustawień). Usuwamy `updateIgnorePatterns` method.

**Lokalizacje:**

- `src/main/fileWatcher/defaults.ts` — rozdziel dwie listy
- `src/main/fileWatcher/FileTreeWatcher.ts` — usuń parameter ignorePatterns z konstruktora

**Zadania:**

- [ ] W `defaults.ts`:
  - `SAFETY_IGNORE_PATTERNS` — hardcoded lista używana NATYWNIE przez watchera (`node_modules`, `.git`, `dist`, `build`, `.next`, `.nuxt`, `.output`, `.svelte-kit`, `out`, `target`, `.venv`, `venv`, `__pycache__`, `.pytest_cache`, `.DS_Store`, `coverage`, `.turbo`, `.cache`, `.parcel-cache`). Eksportuj jako `readonly string[]`.
  - `DEFAULT_USER_IGNORE_PATTERNS` — identyczna lista zwracana jako "reset to defaults" dla użytkownika (przez `files:getDefaultIgnorePatterns` IPC). Może być ten sam import.
  - Kolekcja jest dwukrotnie użyta: (1) natywnie przez watcher, (2) jako default w user prefs. Trzymamy je razem żeby uniknąć rozsynchronizacji.
- [ ] W `FileTreeWatcher.ts`:
  - Usuń parametr `ignorePatterns` z konstruktora.
  - Usuń pole `private ignorePatterns: string[]`.
  - Usuń metodę `updateIgnorePatterns()`.
  - W `start()` używaj `SAFETY_IGNORE_PATTERNS` jako `ignore` option.
  - `import { SAFETY_IGNORE_PATTERNS } from './defaults'`

**Wzorce do zastosowania:**

```ts
// defaults.ts
export const SAFETY_IGNORE_PATTERNS: readonly string[] = [...]
export const DEFAULT_USER_IGNORE_PATTERNS = SAFETY_IGNORE_PATTERNS

// FileTreeWatcher.ts
constructor(
  private readonly repoRoot: string,
  private readonly onChange: (events: FileChangeEvent[]) => void,
) {}

start(): ResultAsync<void, FileWatcherError> {
  if (this.subscription) return okAsync(undefined)
  return fromExternalCall(
    watcher.subscribe(this.repoRoot, this.handleEvents, {
      ignore: [...SAFETY_IGNORE_PATTERNS],
    }),
    ...
  ).map((sub) => { this.subscription = sub })
}
```

**Tech notes:**

- **Gotcha:** `SAFETY_IGNORE_PATTERNS` to `readonly string[]`, `parcel/watcher` chce mutable `string[]`. Używaj `[...SAFETY_IGNORE_PATTERNS]` przy wywołaniu.
- **Edge case:** user może chcieć widzieć `node_modules` w sidebarze przez usunięcie go z ignorów. Teraz tego NIE POTRAFI — watcher safety go ignoruje. To jest akceptowalny tradeoff (nie chcemy explozji eventów przy `npm install`). Udokumentować w komentarzu.
- **Walidacja:** `npm run typecheck:node` — musi przejść po usunięciu `updateIgnorePatterns`.

---

### Faza 2: Uprość IPC handlers

**Cel:** `files:watch` nie przekazuje już user patterns. `files:updateIgnorePatterns` tylko zapisuje prefs (nie restartuje watchera). `files:getDefaultIgnorePatterns` pozostaje bez zmian.

**Lokalizacje:**

- `src/main/ipc/handlers.ts` — `files:watch`, `files:updateIgnorePatterns`, `getIgnorePatterns()`

**Zadania:**

- [ ] Usuń wywołanie `const patterns = getIgnorePatterns()` z `files:watch`.
- [ ] `new FileTreeWatcher(payload.repoRoot, (events) => {...})` (bez patterns w konstruktorze).
- [ ] `files:updateIgnorePatterns` — usuń restart logic (iterację po `getAllFileWatchers`). Zostaw tylko `preferencesStore.set(...)`.
- [ ] Usuń `getAllFileWatchers()` method z `WindowManager` jeśli nie używana nigdzie indziej (sprawdź). Jeśli tylko tutaj była — usuń.
- [ ] `getIgnorePatterns()` helper pozostaje — używa go `fs:readDir` dla sidebar filtering.
- [ ] `isIgnoredEntry()` helper pozostaje.

**Wzorce do zastosowania:**

```ts
ipcMain.handle('files:watch', async (event, payload: { repoRoot: string }) => {
  if (typeof payload?.repoRoot !== 'string' || !path.isAbsolute(payload.repoRoot)) {
    throw new Error('Invalid repoRoot: must be an absolute path string')
  }
  await validatePathAccess(event.sender.id, payload.repoRoot)

  const senderId = event.sender.id
  windowManager.disposeFileWatcher(senderId)

  const watcher = new FileTreeWatcher(payload.repoRoot, (events) => {
    if (!event.sender.isDestroyed()) {
      event.sender.send('files:changed', { repoRoot: payload.repoRoot, events })
    }
  })

  const result = await watcher.start()
  if (result.isErr()) throw new Error(fileWatcherErrorMessage(result.error))
  windowManager.setFileWatcher(senderId, watcher)
})

ipcMain.handle('files:updateIgnorePatterns', async (_event, payload: { patterns: unknown }) => {
  const patterns = validatePatternsPayload(payload?.patterns)
  preferencesStore.set('files.ignorePatterns', JSON.stringify(patterns))
  // Watcher no longer needs to restart — patterns only affect sidebar rendering now
})
```

**Tech notes:**

- **Gotcha:** jeśli `getAllFileWatchers()` jest używana gdzie indziej, nie usuwaj. Sprawdź przez grep.
- **Kompatybilność:** `files:updateIgnorePatterns` nadal istnieje i przyjmuje payload — nie łamie preload API. Tylko zachowanie się zmienia (nie ma już restartu watchera).
- **Walidacja:** typecheck + manual test: otwórz settings, zmień patterns, sprawdź że sidebar się przeczyścia po changed event watchera (bo renderer filtruje teraz live).

---

### Faza 3: Filter w rendererze — `fileTree.applyFileEvents` aplikuje user patterns

**Cel:** Skoro watcher emituje wszystko co nie jest safety-ignored, renderer musi sam filtrować user patterns przed refresh'owaniem folderów.

**Lokalizacje:**

- `src/renderer/src/lib/stores/fileTree.svelte.ts` — extend `applyFileEvents`

**Zadania:**

- [ ] Zaimportuj `prefs` z `preferences.svelte` (już może być zaimportowane — sprawdź).
- [ ] Parse `prefs['files.ignorePatterns']` (JSON string → array). Jeśli brak — pusty array.
- [ ] Helper `isIgnoredByUser(relPath: string, patterns: string[]): boolean` — ten sam algo co `isIgnoredEntry` w main (plain names → exact match, pierwszy segment globa). Możesz skopiować logikę (nie współdzielimy kodu main/renderer).
- [ ] W `applyFileEvents`: przed dodaniem parentDir do `dirsToRefresh`, sprawdź czy ścieżka eventu NIE matchuje user ignore — jeśli matchuje, skip.
- [ ] UWAGA: `fs:readDir` już filtruje user patterns — więc nawet bez filtra w renderze, pliki by się nie pojawiły. Ale ponowny refresh foldera jest marnotrawstwem. Filter oszczędza cycle.

**Wzorce do zastosowania:**

```ts
function getUserIgnorePatterns(): string[] {
  const raw = prefs['files.ignorePatterns']
  if (!raw) return []
  try {
    const parsed = JSON.parse(raw)
    return Array.isArray(parsed) && parsed.every((p) => typeof p === 'string') ? parsed : []
  } catch {
    return []
  }
}

function isIgnoredByUser(relPath: string, patterns: string[]): boolean {
  // Check each path segment against simple-name patterns
  const segments = relPath.split('/')
  for (const pattern of patterns) {
    if (!pattern.includes('*') && !pattern.includes('?') && !pattern.includes('/')) {
      if (segments.includes(pattern)) return true
      continue
    }
    const firstSegment = pattern.split('/')[0]
    if (firstSegment && !firstSegment.includes('*') && segments.includes(firstSegment)) {
      return true
    }
  }
  return false
}

function applyFileEvents(events: FileChangeEvent[]): void {
  if (!rootPath) return
  const root = rootPath
  const userPatterns = getUserIgnorePatterns()

  const dirsToRefresh = new SvelteSet<string>()
  for (const ev of events) {
    if (isIgnoredByUser(ev.path, userPatterns)) continue
    const absPath = `${root}/${ev.path}`
    const lastSlash = absPath.lastIndexOf('/')
    if (lastSlash === -1) continue
    const parent = absPath.substring(0, lastSlash)
    if (expandedDirs[parent]) {
      dirsToRefresh.add(parent)
    }
  }
  if (dirsToRefresh.size === 0) return
  void Promise.all([...dirsToRefresh].map((dir) => expandDir(dir)))
}
```

**Tech notes:**

- **Referencja:** helper `isIgnoredEntry` w `src/main/ipc/handlers.ts` — wzoruj na tej samej logice, ale operuj na całej ścieżce (`rel.path` może mieć wiele segmentów), nie tylko na nazwie.
- **Różnica:** w main `isIgnoredEntry` działa na nazwie bezpośredniego dziecka (bo `readDir` pokazuje tylko dzieci). Tutaj mamy pełną relative path (`tmp/subdir/file.log`), więc sprawdzamy każdy segment.
- **Gotcha:** użyj `SvelteSet` nie natywnego `Set` — lint rule `svelte/prefer-svelte-reactivity`.
- **Walidacja:** dodaj `tmp/**` do Settings, utwórz `tmp/x.txt` w terminalu — sidebar NIE powinien zareagować na event.

---

### Faza 4: Migracja `GitWatcher` na `@parcel/watcher`

**Cel:** Wywalić chokidar z repo. GitWatcher używa tej samej biblioteki co FileTreeWatcher.

**Lokalizacje:**

- `src/main/git/GitWatcher.ts` — refactor

**Zadania:**

- [ ] Usuń `import { watch } from 'chokidar'` i `type { FSWatcher }`.
- [ ] Dodaj `import * as watcher from '@parcel/watcher'`, `import { fromExternalCall, errorMessage } from '../errors'`.
- [ ] Zmień `private watcher: FSWatcher | null = null` na `private subscription: watcher.AsyncSubscription | null = null`.
- [ ] `start()`:
  - Obserwuj `.git/` directory (nie pojedyncze pliki — parcel/watcher obserwuje dir)
  - W handlerze filtruj eventy które dotyczą HEAD/index/refs/worktrees
  - `parcel/watcher` zwraca Promise → `start()` musi być async lub zwracać Promise
- [ ] `stop()`:
  - `await this.subscription?.unsubscribe()`
- [ ] `handleEvents` callback:
  - Dla każdego eventu sprawdź czy path dotyczy HEAD/index/refs/worktrees
  - Jeśli tak, wywołaj istniejącą `markPendingRefresh(changedPath)` + scheduleRefresh
  - Usuń helper `scheduleRefresh` (zmieniamy na bezpośrednie wywołanie pending refresh?) — lub zostawimy scheduleRefresh logic ale wywołujemy go ręcznie
- [ ] UWAGA: GitWatcher obecnie jest synchroniczny w `start()`. Trzeba to zmienić na async lub fire-and-forget.

**Wzorce do zastosowania:**

```ts
import * as watcher from '@parcel/watcher'
import { fromExternalCall, errorMessage } from '../errors'

export class GitWatcher {
  private subscription: watcher.AsyncSubscription | null = null
  private debounceTimer: ReturnType<typeof setTimeout> | null = null
  private readonly debounceMs = 300
  private pendingRefresh: GitRefreshFlags = { ... }
  // ...

  async start(): Promise<void> {
    if (this.subscription) return
    const gitDir = join(this.repoRoot, '.git')
    try {
      this.subscription = await watcher.subscribe(
        gitDir,
        this.handleEvents,
        { ignore: ['objects', 'logs', 'hooks'] }, // skip write-heavy dirs
      )
    } catch (e) {
      console.warn(`[GitWatcher] failed to start for ${gitDir}:`, errorMessage(e))
    }
  }

  async stop(): Promise<void> {
    if (this.debounceTimer) { clearTimeout(this.debounceTimer); this.debounceTimer = null }
    const sub = this.subscription
    if (!sub) return
    this.subscription = null
    try { await sub.unsubscribe() } catch { /* ignore */ }
  }

  private handleEvents = (err: Error | null, events: watcher.Event[]): void => {
    if (err) {
      console.warn(`[GitWatcher] native error:`, err.message)
      return
    }
    for (const ev of events) {
      // only care about HEAD/index/refs/worktrees
      this.scheduleRefresh(ev.path)
    }
  }

  // Reuse existing scheduleRefresh / markPendingRefresh / consumePendingRefresh / refreshInfo
}
```

**Tech notes:**

- **Gotcha #1:** `start()` teraz jest async. Wywołania w `handlers.ts git:watch` muszą być `await`owane. Sprawdź istniejący handler i dostosuj.
- **Gotcha #2:** chokidar używa `awaitWriteFinish` — parcel/watcher nie ma odpowiednika. Istniejący `debounceMs = 300` i tak to kompensuje.
- **Gotcha #3:** parcel/watcher obserwuje CAŁY `.git/` katalog. To generuje dużo eventów (objects, logs). Ignoruj natywnie: `{ ignore: ['objects', 'logs', 'hooks'] }` — redukuje noise.
- **Gotcha #4:** `markPendingRefresh` sprawdza ścieżki absolute vs relative. `parcel/watcher` zwraca absolute. Dostosuj jeśli trzeba.
- **Referencja:** `src/main/fileWatcher/FileTreeWatcher.ts` — wzoruj strukturę (subscription, handleEvents, fromExternalCall). Ale `GitWatcher` ma bardziej złożoną logikę pending refresh — zachowaj ją.
- **Walidacja:** przełącz branch w terminalu — diff pane powinien zareagować (tak jak teraz); commit w terminalu — też powinien reagować.

---

### Faza 5: `DiffPane` + `ChangesPanel` subskrybują `onFilesChanged`

**Cel:** Oba komponenty reagują na realne zmiany plików z debounce 200ms, niezależnie od git metadata.

**Lokalizacje:**

- `src/renderer/src/components/diff/DiffPane.svelte`
- `src/renderer/src/components/diff/ChangesPanel.svelte`

**Zadania:**

- [ ] W obu komponentach, w `onMount` dodaj subskrypcję `onFilesChanged`:
  ```ts
  let fileRefreshTimer: ReturnType<typeof setTimeout> | null = null
  const unsubFiles = window.api.onFilesChanged((payload) => {
    if (payload.repoRoot !== worktreePath) return
    if (fileRefreshTimer) clearTimeout(fileRefreshTimer)
    fileRefreshTimer = setTimeout(() => {
      fileRefreshTimer = null
      refresh().then(() => triggerPulse?.())
    }, 200)
  })
  ```
- [ ] Zaktualizuj cleanup w `onMount` żeby wywołał też `unsubFiles()` i `clearTimeout(fileRefreshTimer)`.
- [ ] UWAGA: `ChangesPanel` nie ma `triggerPulse` — tylko `DiffPane` ma.
- [ ] UWAGA: `worktreePath` comparison — `payload.repoRoot` to path, który przekazaliśmy do `watchFiles`. W workspace.svelte.ts już wywoływaliśmy `watchFiles(path)` z path wybranego worktree, więc to powinno się zgadzać.

**Wzorce do zastosowania:**

```svelte
onMount(() => {
  const unsubGit = window.api.onGitChanged(() => {
    refresh().then(() => triggerPulse())
  })

  let fileRefreshTimer: ReturnType<typeof setTimeout> | null = null
  const unsubFiles = window.api.onFilesChanged((payload) => {
    if (payload.repoRoot !== worktreePath) return
    if (fileRefreshTimer) clearTimeout(fileRefreshTimer)
    fileRefreshTimer = setTimeout(() => {
      fileRefreshTimer = null
      refresh().then(() => triggerPulse())
    }, 200)
  })

  // ... reszta istniejącego onMount

  return () => {
    unsubGit()
    unsubFiles()
    if (fileRefreshTimer) clearTimeout(fileRefreshTimer)
    // ... reszta istniejącego cleanup
  }
})
```

**Tech notes:**

- **Referencja:** wzoruj na istniejącej subskrypcji `onGitChanged` w tym samym komponencie (linie 87-90 DiffPane, 42-49 ChangesPanel).
- **Gotcha:** `onFilesChanged` wysyła `{ repoRoot, events }` — filtruj po `repoRoot === worktreePath`. W przeciwnym razie wieloworktree workspace dostanie krzyżowe eventy.
- **Edge case:** worktreePath może się zmienić przez reaktywność props. Wtedy subskrypcja w onMount NIE jest reaktywna — `worktreePath` w closure to stale value. Rozwiązanie: użyj `$effect` zamiast `onMount`, albo korzystaj z istniejącego `$effect(() => { void worktreePath; refresh() })` który już reaguje na zmianę. ACTUALLY — eventy przylatują z main via IPC push, i dla nowego worktree wywołany jest `watchFiles(newPath)` przez workspace.svelte.ts, co disposes poprzedni watcher. Więc stare eventy nie będą przylatywać. Ale stale closure w callback pozostaje — jeśli user przełączy worktree, callback porównuje `payload.repoRoot !== OLD worktreePath`. Bezpieczniej: umieść subskrypcję w `$effect` który reaguje na `worktreePath`.
- **Performance:** debounce 200ms = maksymalnie 5 refresh/s. Refresh jest kosztowny (git diff HEAD + ls-files + parse). Przy rapidnych zmianach (jedna paczka plików od agenta) — 200ms bufferuje burst do pojedynczego refresh.
- **Walidacja:** utwórz plik w terminalu — diff pane powinien pokazać go w < 300ms. Usuń plik — znika. Zrób `echo "x" >> existing.txt` — diff się aktualizuje.

---

### Faza 6: Cleanup — usuń `chokidar`, update docs/comments

**Cel:** Odchudzić dependencies, zaktualizować komentarze które wspominają chokidar.

**Lokalizacje:**

- `package.json` — remove `chokidar`
- `src/main/fileWatcher/defaults.ts` — update jsdoc
- `src/main/git/GitWatcher.ts` — usuń chokidar z komentarzy

**Zadania:**

- [ ] `npm uninstall chokidar` (jeśli nie jest używany gdzie indziej — sprawdź `rg chokidar src/`).
- [ ] Jeśli chokidar jest tylko w `GitWatcher` i `FileTreeWatcher` (stary) — uninstall jest bezpieczny.
- [ ] `defaults.ts` — upewnij się że jsdoc wyjaśnia rozróżnienie `SAFETY_IGNORE_PATTERNS` (natywnie watched) vs `DEFAULT_USER_IGNORE_PATTERNS` (default dla settings).
- [ ] `GitWatcher.ts` — usuń jakiekolwiek wspomnienie chokidar w komentarzach, zastąp jsdoc nawiązując do parcel/watcher.

**Tech notes:**

- **Walidacja:** `rg chokidar` w `src/` musi zwrócić 0. `npm run build` musi przejść.
- **Gotcha:** `@parcel/watcher` ma transitive `chokidar`? Sprawdź `package-lock.json`. Jeśli tak, nie mogę go całkowicie usunąć z node_modules, ale z direct deps (`package.json`) — tak.

---

### Faza 7: Weryfikacja końcowa

**Cel:** Lint, typecheck, build, manual tests.

**Zadania:**

- [ ] `npm run lint` — 0 errors
- [ ] `npm run typecheck` — 0 errors
- [ ] `npm run build` — successful
- [ ] `rg chokidar src/` — 0 matches
- [ ] Manual tests (macOS):
  - [ ] Otwórz workspace, przełącz na tab Diff
  - [ ] W terminalu: `echo "x" > new-file.md` — pojawia się w diff pane w < 300ms
  - [ ] `rm new-file.md` — znika z diff pane
  - [ ] `echo "y" >> existing.txt` — diff się aktualizuje (content + hunks)
  - [ ] Claude Code tworzy plik → diff pokazuje go od razu
  - [ ] Branch switch w terminalu (`git checkout -b test`) — GitWatcher nadal działa, diff się aktualizuje
  - [ ] `git stash`, `git stash pop` — diff pane reaguje
  - [ ] Dodaj `tmp/**` do Settings → File Watcher → utwórz `tmp/x.txt` — **diff pane POKAZUJE** (bo user patterns nie wpływają), ale sidebar NIE pokazuje
  - [ ] Usuń `node_modules` z Settings — sidebar nadal go nie pokazuje (safety ignore w watcherze zapobiega nawet emitowaniu eventów)
  - [ ] `npm install` — UI nie zamiera, diff się nie aktualizuje miliony razy (safety ignores chronią)
  - [ ] Przełącz worktree — watcher restartuje się, diff reaguje na nowy worktree

**Kryteria akceptacji:**

- [ ] Diff pane pokazuje nowe pliki bez czekania na git operation
- [ ] User ignore patterns z Settings NIE wpływają na zawartość diff pane
- [ ] Sidebar nadal respektuje user ignore patterns (przez filter w renderze)
- [ ] `chokidar` usunięty z `package.json` dependencies
- [ ] GitWatcher używa `@parcel/watcher`
- [ ] Manual testy przechodzą
