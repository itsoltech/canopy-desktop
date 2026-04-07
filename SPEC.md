# SPEC: Repo-level Task Tracker Configuration

## Wymagania

- **Konfiguracja w repozytorium**: branch templates, PR templates i tracker connection przechowywane w `.canopy/config.json` w roocie repo
- **Plik commitowany**: współdzielony z zespołem (wchodzi do git)
- **Jedno połączenie tracker, wiele boardów**: config definiuje jedną connection (provider, URL, project) z opcjonalnymi per-board overrides template'ów
- **Tokeny w OS keychain**: identyfikowane po `provider + baseUrl` (np. `canopy:jira:https://mycompany.atlassian.net`). User dodaje token raz — działa dla każdego repo z tym samym tracker URL
- **Repo config wygrywa**: jeśli istnieje `.canopy/config.json`, Canopy prefs są ignorowane dla tego repo
- **Brak migracji**: starą konfigurację porzucamy, wymuszamy nową wersję
- **UI bez zmian koncepcyjnych**: istniejące Preferences teraz edytują `.canopy/config.json` zamiast Canopy prefs — zapis = zapis pliku
- **Missing token flow**: toast notification "Tracker requires authentication" z linkiem do ustawień

## Code Style & Konwencje projektu

**Źródła:** `CLAUDE.md`, `.prettierrc`, `eslint.config.mjs`

**Kluczowe konwencje:**

- Single quotes, no semicolons, 100 char print width, 2-space indent
- `import type` dla type-only imports (`verbatimModuleSyntax`)
- Custom form components: `CustomSelect`, `CustomCheckbox`, `CustomRadio` z `shared/`
- IPC: `ipcMain.handle('channel', handler)` → `ipcRenderer.invoke('channel', payload)`
- Electron `safeStorage` do szyfrowania wrażliwych danych

## Analiza złożoności

- **Scope:** duży — zmiana fundamentalnego podejścia do storage konfiguracji
- **Szacowana liczba plików:** ~15
- **Główne obszary:**
  - Nowy moduł: `RepoConfigManager` (odczyt/zapis `.canopy/config.json`)
  - Nowy moduł: `KeychainTokenStore` (OS keychain zamiast encrypted prefs)
  - Refactor: `TaskTrackerManager` (czyta z repo config zamiast prefs)
  - Refactor: IPC handlers (nowe kanały dla repo config + keychain)
  - Refactor: UI Preferences (edytuje plik zamiast prefs)
  - Refactor: Preload API (nowe metody)

---

## `.canopy/config.json` — docelowy format

```jsonc
{
  "version": 1,
  "tracker": {
    "provider": "jira",
    "baseUrl": "https://mycompany.atlassian.net",
    "projectKey": "GAKKO",
    "username": "user@example.com", // opcjonalne (Jira Cloud wymaga)
    "defaultBoardId": "42", // opcjonalny domyślny board
  },
  "branchTemplate": {
    "template": "{branchType}/{taskKey}-{taskTitle}",
    "customVars": { "team": "frontend" },
    "typeMapping": {
      "bug": "fix",
      "story": "feat",
    },
  },
  "prTemplate": {
    "titleTemplate": "[{taskKey}] {taskTitle}",
    "bodyTemplate": "## {taskKey}: {taskTitle}\n\n{taskUrl}",
    "defaultTargetBranch": "develop",
    "targetRules": [{ "taskType": "bug", "targetPattern": "main" }],
  },
  "boardOverrides": {
    "55": {
      "branchTemplate": {
        "template": "{branchType}/{boardKey}-{taskKey}-{taskTitle}",
      },
      "prTemplate": {
        "defaultTargetBranch": "main",
      },
    },
  },
  "filters": {
    "assignedToMe": true,
    "statuses": ["To Do", "In Progress"],
  },
}
```

---

## Keychain token — schemat klucza

Format: `canopy:{provider}:{baseUrl}`
Przykład: `canopy:jira:https://mycompany.atlassian.net`

- User dodaje token raz per provider+URL
- Działa automatycznie dla każdego repo z tym samym tracker
- Na Windows: Windows Credential Manager (via `keytar` lub Electron `safeStorage` z dedykowanym kluczem)
- Na macOS: Keychain
- Na Linux: libsecret / gnome-keyring

---

## Plan implementacji

### Faza 1: RepoConfigManager — odczyt/zapis `.canopy/config.json`

**Cel:** Moduł do zarządzania plikiem konfiguracyjnym w repozytorium.

**Lokalizacje:**

- `src/main/taskTracker/RepoConfigManager.ts` — **nowy plik**

**Zadania:**

- [ ] Stwórz klasę `RepoConfigManager` z metodami:
  - `load(repoRoot: string): RepoConfig | null` — odczyt i parsowanie `.canopy/config.json`
  - `save(repoRoot: string, config: RepoConfig): void` — zapis z pretty-print JSON
  - `exists(repoRoot: string): boolean` — sprawdza czy plik istnieje
  - `init(repoRoot: string): RepoConfig` — tworzy `.canopy/` folder + domyślny `config.json`
- [ ] Zdefiniuj interfejs `RepoConfig` (odpowiada formatowi JSON powyżej)
- [ ] Walidacja wersji — `version: 1`
- [ ] Metody do resolve scope: `getBranchTemplate(boardId?)`, `getPRTemplate(boardId?)` — sprawdza `boardOverrides` potem domyślne

**Wzorce do zastosowania:**

```typescript
// Wzoruj się na sync I/O pattern jak w PreferencesStore
// fs.readFileSync / fs.writeFileSync — config czytany rzadko, nie warto async
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs'
import { join } from 'path'

const CONFIG_DIR = '.canopy'
const CONFIG_FILE = 'config.json'
```

**Tech notes:**

- **Gotcha:** repoRoot może nie istnieć (nowe workspace bez git) → `exists()` musi to obsłużyć
- **Gotcha:** plik JSON może być ręcznie edytowany z błędami → graceful error handling
- **Edge case:** `.canopy/` folder istnieje ale bez `config.json`
- **Walidacja:** `load()` + `save()` + `load()` roundtrip bez utraty danych

---

### Faza 2: KeychainTokenStore — tokeny w OS keychain

**Cel:** Zastąpić `taskTracker.token.*` w PreferencesStore na Electron `safeStorage` z dedykowanymi kluczami per provider+URL.

**Lokalizacje:**

- `src/main/taskTracker/KeychainTokenStore.ts` — **nowy plik**

**Zadania:**

- [ ] Stwórz klasę `KeychainTokenStore` z metodami:
  - `getToken(provider: string, baseUrl: string): string | null`
  - `setToken(provider: string, baseUrl: string, token: string): void`
  - `deleteToken(provider: string, baseUrl: string): void`
  - `hasToken(provider: string, baseUrl: string): boolean`
- [ ] Klucz keychain: `canopy:{provider}:{baseUrl}` (np. `canopy:jira:https://x.atlassian.net`)
- [ ] Użyj Electron `safeStorage` — ten sam mechanizm co teraz, ale z innym schematem kluczy
- [ ] Tokeny przechowywane w tabeli `preferences` z prefiksem `keychain.` (zaszyfrowane via safeStorage)

**Wzorce do zastosowania:**

```typescript
// Wzoruj się na istniejący PreferencesStore encrypted keys pattern
// safeStorage.encryptString() / decryptString()
// Klucz w DB: keychain.canopy:jira:https://... → zaszyfrowana wartość
```

**Tech notes:**

- **Gotcha:** `baseUrl` może mieć trailing slash lub nie — normalizuj przed użyciem jako klucz
- **Gotcha:** `safeStorage.isEncryptionAvailable()` może zwrócić false na niektórych Linux distro
- **Kompatybilność:** Stare tokeny `taskTracker.token.*` — nie migrujemy, user musi podać ponownie
- **Security:** Tokeny nigdy nie opuszczają procesu main — preload nie ma do nich dostępu
- **Walidacja:** set → get roundtrip zwraca ten sam token

---

### Faza 3: Refactor TaskTrackerManager

**Cel:** TaskTrackerManager czyta connection z repo config zamiast z prefs. Token pobiera z KeychainTokenStore.

**Zależy od:** Faza 1, 2

**Lokalizacje:**

- `src/main/taskTracker/TaskTrackerManager.ts` — refactor

**Zadania:**

- [ ] Zmień konstruktor — przyjmuje `KeychainTokenStore` zamiast `PreferencesStore`
- [ ] Usuń `getConnections()` / `saveConnections()` / `addConnection()` / `removeConnection()` / `updateConnection()` — connections żyją teraz w repo configu
- [ ] Nowe metody operujące na repo config:
  - `getConnectionFromConfig(config: RepoConfig): TaskTrackerConnection` — buduje obiekt connection z repo configu + token z keychain
  - `testConnectionFromConfig(config: RepoConfig): Promise<boolean>`
- [ ] `getToken()` zmieniony — pobiera z `KeychainTokenStore` po `provider + baseUrl`
- [ ] Metody fetch (boards, tasks, statuses, sprints) — przyjmują `RepoConfig` jako kontekst
- [ ] `findTaskByKey()` — pobiera connection z active repo config

**Wzorce do zastosowania:**

```typescript
// Nowy flow:
// 1. IPC handler dostaje repoRoot z payload
// 2. RepoConfigManager.load(repoRoot) → config
// 3. TaskTrackerManager.getConnectionFromConfig(config) → connection (z tokenem)
// 4. Provider client wykonuje operację
```

**Tech notes:**

- **Breaking change:** `TaskTrackerManager` nie zarządza już CRUD connections — to robi `RepoConfigManager`
- **Gotcha:** Connection z config nie ma `id` ani `authPrefKey` — trzeba zmienić `TaskTrackerConnection` interface lub stworzyć nowy
- **Edge case:** Config istnieje ale brak tokena w keychain → throw z jasnym komunikatem
- **Walidacja:** fetch tasks z repo config działa tak samo jak wcześniej z prefs

---

### Faza 4: Nowe IPC handlery + preload API

**Cel:** Zastąpić/dodać IPC kanały dla nowego systemu.

**Zależy od:** Fazy 1-3

**Lokalizacje:**

- `src/main/ipc/handlers.ts` — refactor sekcji task tracker (~lines 942-1347)
- `src/preload/index.ts` — nowe metody API
- `src/preload/index.d.ts` — typy

**Zadania:**

- [ ] Nowe IPC handlery:
  - `repoConfig:load` (repoRoot) → `RepoConfig | null`
  - `repoConfig:save` (repoRoot, config) → void
  - `repoConfig:exists` (repoRoot) → boolean
  - `repoConfig:init` (repoRoot) → RepoConfig
  - `keychain:hasToken` (provider, baseUrl) → boolean
  - `keychain:setToken` (provider, baseUrl, token) → void
  - `keychain:deleteToken` (provider, baseUrl) → void
- [ ] Refactor istniejących handlerów:
  - `taskTracker:resolveBranchName` — czyta template z repo config (payload musi zawierać `repoRoot`)
  - `taskTracker:createPR` — analogicznie
  - `taskTracker:fetchTasks/Boards/Statuses` — czyta connection z repo config
  - `taskTracker:testConnection` — testuje na podstawie repo config + keychain token
- [ ] Usuń stare handlery:
  - `taskTracker:addConnection` / `removeConnection` / `updateConnection` — CRUD jest teraz w repo config
- [ ] Preload API — nowe metody:
  - `repoConfigLoad(repoRoot)`, `repoConfigSave(repoRoot, config)`, `repoConfigExists(repoRoot)`, `repoConfigInit(repoRoot)`
  - `keychainHasToken(provider, baseUrl)`, `keychainSetToken(...)`, `keychainDeleteToken(...)`
- [ ] Typy w `index.d.ts`:
  - `RepoConfig` interface
  - Nowe metody w `CanopyAPI`

**Wzorce do zastosowania:**

```typescript
// Wzoruj się na istniejące handler pattern
ipcMain.handle('repoConfig:load', (_event, payload: { repoRoot: string }) => {
  return repoConfigManager.load(payload.repoRoot)
})
```

**Tech notes:**

- **Gotcha:** `repoRoot` musi być dostępny w payload — renderer zna go z `workspaceState`
- **Security:** `keychain:setToken` musi walidować że provider/baseUrl nie są puste
- **Kompatybilność:** Stare handlery `taskTracker:getConnections` etc. — usunąć, stary system nie jest wspierany
- **Walidacja:** Pełny roundtrip: renderer → save config → load config → dane zgodne

---

### Faza 5: Refactor UI — TaskConnectionsPrefs

**Cel:** Zmiana formularza connections — zamiast CRUD wielu connections, edycja jednego tracker z repo configu. Token flow przez keychain.

**Zależy od:** Faza 4

**Lokalizacje:**

- `src/renderer/src/components/preferences/TaskConnectionsPrefs.svelte` — refactor
- `src/renderer/src/lib/stores/taskTracker.svelte.ts` — refactor store

**Zadania:**

- [ ] Zmień UI z listy connections na single connection editor:
  - Provider select (Jira / YouTrack)
  - Base URL input
  - Project key input
  - Username input (opcjonalny, zależy od provider)
  - Default board dropdown
  - Token input — zapisuje do keychain, nie do config
- [ ] Token status indicator: "authenticated" / "token required"
- [ ] Test connection button — używa config + token z keychain
- [ ] Zapis → `repoConfigSave()` (plik `.canopy/config.json`)
- [ ] Missing token notification:
  - Przy otwarciu repo z config bez tokena → toast "Tracker requires authentication" z linkiem do prefs
- [ ] Refactor `taskTracker.svelte.ts` store:
  - Zamiast `loadConnections()` z prefs → `loadRepoConfig(repoRoot)` z pliku
  - State: `repoConfig: RepoConfig | null`, `hasToken: boolean`

**Tech notes:**

- **Gotcha:** `CustomSelect` wymaga `options` jako `{ value: string, label: string }[]`
- **Gotcha:** Board list wymaga tokena — fetch boards dopiero po podaniu tokena
- **Edge case:** Nowe repo bez `.canopy/config.json` — pokaż "Initialize" button
- **Edge case:** Token w keychain ale config zmieniony (inny baseUrl) — pokaż token required
- **Walidacja:** Pełny flow: edytuj config → save → reload → dane poprawne

---

### Faza 6: Refactor UI — TaskBranchNamingPrefs i TaskPRNamingPrefs

**Cel:** Template editors edytują repo config zamiast prefs. Scope uproszczony do default + per-board.

**Zależy od:** Faza 5

**Lokalizacje:**

- `src/renderer/src/components/preferences/TaskBranchNamingPrefs.svelte` — refactor
- `src/renderer/src/components/preferences/TaskPRNamingPrefs.svelte` — refactor
- `src/renderer/src/components/preferences/TaskTrackerPrefs.svelte` — refactor parent

**Zadania:**

- [ ] Branch naming:
  - Scope dropdown: "Default" / per-board (zamiast per-connection/per-board)
  - Template editor bez zmian (BranchTokenBuilder)
  - Zapis do `repoConfig.branchTemplate` lub `repoConfig.boardOverrides[boardId].branchTemplate`
  - Type mapping editor → `repoConfig.branchTemplate.typeMapping`
- [ ] PR naming:
  - Analogicznie — scope "Default" / per-board
  - Title/body template builders
  - Target branch + target rules
  - Zapis do `repoConfig.prTemplate` lub `boardOverrides`
- [ ] TaskTrackerPrefs parent:
  - Zamiast `exportConfig` / `importConfig` — nie potrzebne, config jest w repo
  - Filters (assignedToMe, statuses) → zapis do `repoConfig.filters`
  - Scope boards ładowane z repo config connection
- [ ] Każdy zapis → `window.api.repoConfigSave(repoRoot, updatedConfig)`

**Tech notes:**

- **Gotcha:** Scope się upraszcza — brak per-connection level (jest tylko jedno połączenie)
- **Gotcha:** `boardOverrides` to partial — merge z defaultami przy odczycie
- **Edge case:** Board override bez template → fallback do domyślnego
- **Walidacja:** Edycja template → zapis → reload pliku → template się zgadza

---

### Faza 7: Auto-detection przy otwarciu repo + cleanup

**Cel:** Canopy automatycznie szuka `.canopy/config.json` przy otwarciu/attach repozytorium. Cleanup starego systemu.

**Zależy od:** Fazy 1-6

**Lokalizacje:**

- `src/renderer/src/lib/stores/workspace.svelte.ts` — hook przy `attachProject`
- `src/renderer/src/lib/stores/taskTracker.svelte.ts` — auto-load
- `src/main/ipc/handlers.ts` — cleanup starych handlerów
- `src/main/db/PreferencesStore.ts` — cleanup encrypted key prefixes

**Zadania:**

- [ ] W `attachProject()` lub po `selectWorktree()`:
  - Sprawdź `repoConfigExists(repoRoot)`
  - Jeśli tak: załaduj config, sprawdź token w keychain
  - Brak tokena → toast notification z linkiem do prefs
- [ ] Cleanup starych prefs keys:
  - Usuń obsługę `taskTracker.connections` w `PreferencesStore`
  - Usuń `taskTracker.token.*` z encrypted key prefixes (nowe tokeny w `keychain.*`)
  - Usuń `taskTracker.branchTemplate.*` i `taskTracker.pr.*` z prefs
- [ ] Cleanup preload:
  - Usuń `taskTrackerAddConnection`, `taskTrackerRemoveConnection`, `taskTrackerUpdateConnection`
  - Usuń `taskTrackerGetConnections`
- [ ] Cleanup IPC handlers — usuń stare handlery
- [ ] Cleanup types — usuń `TaskTrackerExportData`, zaktualizuj `TaskTrackerConfig`

**Tech notes:**

- **Gotcha:** Nie usuwaj istniejących danych z DB — user może mieć stare config i chcieć je ręcznie przenieść
- **Gotcha:** Toast notification — nie spamuj przy każdym switch worktree, pokaż raz per session
- **Edge case:** Repo bez `.canopy/config.json` — sidebar TaskTrackerSection pokazuje "Configure tracker" button
- **Walidacja:** Otwórz repo z config → tracker działa automatycznie. Otwórz repo bez config → brak erroru, tracker section zachęca do konfiguracji

---

## Weryfikacja końcowa

**Testy do wykonania:**

- [ ] `npm run typecheck` — brak błędów typów
- [ ] `npm run lint` — brak błędów ESLint
- [ ] `npm run svelte-check` — brak błędów Svelte
- [ ] Test manualny: nowe repo → "Initialize" → edycja config → zapis → plik `.canopy/config.json` poprawny
- [ ] Test manualny: repo z istniejącym config → auto-detect → tracker działa
- [ ] Test manualny: brak tokena → toast notification → dodanie tokena w prefs → tracker działa
- [ ] Test manualny: per-board override → poprawny template
- [ ] Test manualny: branch creation z repo config
- [ ] Test manualny: PR creation z repo config
- [ ] Test manualny: zmiana board → override aplikowany

**Kryteria akceptacji:**

- [ ] `.canopy/config.json` jest jedynym źródłem konfiguracji tracker/branch/PR
- [ ] Tokeny w OS keychain (via safeStorage), identyfikowane po provider+baseUrl
- [ ] Config commitowalny — brak wrażliwych danych w pliku
- [ ] UI Preferences edytuje plik, nie Canopy prefs
- [ ] Auto-detection przy otwarciu repo
- [ ] Per-board overrides działają
- [ ] Brak regresji: branch creation, PR creation, task picker
