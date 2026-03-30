# SPEC: Issue Tracker Integrations

## Wymagania

### Zebrane podczas interview

- **Integracje:** Jira + YouTrack (architektura pluginowa dla przyszlych providerow)
- **Auth:** API token przechowywany w `safeStorage` (zaszyfrowany w DB jak `claude.apiKey`)
- **UI:** Nowa sekcja sidebar `issues` — CollapsibleSection, konfigurowalna visibility/ordering
- **Branch naming:** Template string z placeholderami + custom stale + live preview
- **Task dialog:** Kompaktowa lista (klucz + tytul + status + priorytet) z wyszukiwarka
- **Branch action:** Utworz branch + checkout (z pytaniem o stash/commit jesli dirty)
- **Task content:** Wstaw tresc taska jako prompt do aktywnej sesji terminala (Claude/Codex)
- **PR naming:** Konfigurowalny template z placeholderami + auto-create via git hosting API
- **PR target:** Konfigurowalna galaz docelowa (develop / story-branch/main)
- **Filtracja taskow:** Pobierz dostepne statusy/kolumny z API boardu, user wybiera ktore fetchowac
- **Przypisanie:** Konfiguracja: tylko moje taski vs wszystkie z wybranych statusow
- **Export/import:** Pelna konfiguracja (bez credentiali) do pliku JSON

## Code Style & Konwencje projektu

**Zrodla:** `CLAUDE.md`, `.prettierrc`, `eslint.config.mjs`, `tsconfig.*.json`

**Kluczowe konwencje:**
- Single quotes, no semicolons, 100 char print width, 2-space indent
- `verbatimModuleSyntax: true` — type-only imports: `import type { X } from '...'`
- Preference keys: `section.subkey` (np. `issueTracker.connections`, `issueTracker.branchTemplate`)
- IPC handler names: `domain:action` (np. `issueTracker:fetchIssues`, `issueTracker:connect`)
- Dialog state: union type + `show{Name}()` function
- CSS: kebab-case classes, `rgba()` colors, z-index 1001 for overlays
- Components: Svelte 5 runes (`$props()`, `$state`, `$derived`, `$effect`)
- Preload API: returns unsubscribe functions for listeners

## Analiza zlozonosci

- **Scope:** Bardzo duzy
- **Szacowana liczba plikow:** ~25-30 nowych + ~10 modyfikacji
- **Glowne obszary:**
  - Main process: issue tracker API clients, IPC handlers
  - Preload: nowe metody API
  - Renderer: sidebar section, modals, preferences panels, stores
  - DB: encrypted credentials, nowe preference keys
- **Zaleznosci:** Provider API clients → IPC handlers → Preload bridge → UI components

---

## Plan implementacji

### Faza 1: Typy i interfejsy bazowe

**Cel:** Zdefiniowac wspolny model danych dla issue trackerow (provider-agnostic)

**Lokalizacje:**
- `src/main/issueTracker/types.ts` — nowy plik z typami

**Zadania:**
- [ ] Zdefiniuj `IssueTrackerProvider` enum: `'jira' | 'youtrack'`
- [ ] Zdefiniuj `IssueTrackerConnection` interface: id, provider, name, baseUrl, projectKey, boardId, auth token ref key
- [ ] Zdefiniuj `TrackerIssue` interface: key, summary, description, status, priority, type (task/story/subtask), parentKey?, sprintName?, sprintNumber?, assignee
- [ ] Zdefiniuj `TrackerBoard` interface: id, name, columns/statuses[]
- [ ] Zdefiniuj `BranchTemplate` interface: template string, custom variables map
- [ ] Zdefiniuj `PRTemplate` interface: titleTemplate, bodyTemplate, targetBranch rules
- [ ] Zdefiniuj `IssueTrackerConfig` interface: connections[], branchTemplate, prTemplate, filters (statusow, assignee mode), exportable shape (bez credentiali)
- [ ] Zdefiniuj `IssueTrackerProviderClient` interface: abstract metody — connect, fetchBoards, fetchStatuses, fetchIssues, getCurrentSprint

**Wzorce do zastosowania:**
```ts
// src/main/issueTracker/types.ts
export type IssueTrackerProvider = 'jira' | 'youtrack'

export interface IssueTrackerConnection {
  id: string
  provider: IssueTrackerProvider
  name: string // display name, np. "Work Jira"
  baseUrl: string // https://mycompany.atlassian.net
  projectKey: string // "ISSUE", "GAKKO"
  boardId?: string
  authPrefKey: string // klucz w PreferencesStore (encrypted)
  username?: string // Jira wymaga email jako username
}

export interface TrackerIssue {
  key: string // "GAKKO-21"
  summary: string
  description: string
  status: string
  priority: string
  type: 'task' | 'story' | 'subtask' | 'bug' | 'epic' | string
  parentKey?: string // dla subtaskow
  sprintName?: string
  sprintNumber?: number
  assignee?: string
}
```

**Tech notes:**
- **Referencja:** wzoruj sie na typach w `src/main/git/types.ts` i `src/main/tools/types.ts`
- **Gotcha:** Jira i YouTrack maja rozne modele danych — interfejsy musza byc dosc generyczne
- **Walidacja:** Typy kompiluja sie bez bledow (`npm run typecheck:node`)

---

### Faza 2: Provider clients — Jira & YouTrack API

**Cel:** Zaimplementowac klientow API dla Jira i YouTrack w main process

**Zalezy od:** Faza 1

**Lokalizacje:**
- `src/main/issueTracker/providers/jira.ts` — Jira REST API client
- `src/main/issueTracker/providers/youtrack.ts` — YouTrack REST API client
- `src/main/issueTracker/providers/index.ts` — factory function

**Zadania:**
- [ ] Implementuj `JiraClient` implementujacy `IssueTrackerProviderClient`:
  - `connect(connection)` — weryfikacja polaczenia (GET /myself)
  - `fetchBoards(projectKey)` — GET /rest/agile/1.0/board?projectKeyOrId=X
  - `fetchStatuses(boardId)` — GET /rest/agile/1.0/board/{id}/configuration (kolumny)
  - `fetchIssues(filters)` — JQL query z filtrami statusow i assignee
  - `getCurrentSprint(boardId)` — GET /rest/agile/1.0/board/{id}/sprint?state=active
- [ ] Implementuj `YouTrackClient` implementujacy `IssueTrackerProviderClient`:
  - `connect(connection)` — GET /api/users/me
  - `fetchBoards(projectKey)` — GET /api/agiles?fields=id,name
  - `fetchStatuses(projectKey)` — GET /api/admin/customFieldSettings/bundles/state
  - `fetchIssues(filters)` — GET /api/issues?query=... z YouTrack query syntax
  - `getCurrentSprint(agileId)` — GET /api/agiles/{id}/sprints?fields=...&$top=1
- [ ] Implementuj `createProviderClient(provider): IssueTrackerProviderClient` factory

**Wzorce do zastosowania:**
```ts
// Uzyj natywnego fetch (dostepny w Node 18+ / Electron)
// Headers: Authorization: Basic (Jira) lub Bearer (YouTrack)
// Error handling: throw typed errors z statusem HTTP i wiadomoscia
```

**Tech notes:**
- **Gotcha:** Jira Cloud vs Server maja rozne endpointy — na razie wspieramy Cloud (REST v3 + Agile)
- **Gotcha:** YouTrack wymaga `Accept: application/json` i `fields=` parametru w kazdym uzyciu
- **Security:** Tokeny nie moga byc logowane — uzyj `[REDACTED]` w error messages
- **Edge cases:** Rate limiting (Jira 429), paginacja (maxResults), timeout
- **Walidacja:** Unit test z mockowanym fetch — poprawne parsowanie odpowiedzi

---

### Faza 3: IPC handlers + Preload bridge

**Cel:** Wystawic operacje issue trackera przez IPC do renderera

**Zalezy od:** Faza 2

**Lokalizacje:**
- `src/main/ipc/handlers.ts` — nowe handlery `issueTracker:*`
- `src/main/issueTracker/IssueTrackerManager.ts` — orchestrator (zarzadza connections, credentials)
- `src/preload/index.ts` — nowe metody w `api` object
- `src/preload/index.d.ts` — typy dla nowych metod

**Zadania:**
- [ ] Utworz `IssueTrackerManager`:
  - Przechowuje aktywne connections (z PreferencesStore)
  - Pobiera tokeny z encrypted prefs
  - Deleguje do odpowiedniego provider clienta
  - Metody: `addConnection`, `removeConnection`, `testConnection`, `getConnections`, `fetchBoards`, `fetchStatuses`, `fetchIssues`, `getCurrentSprint`
- [ ] Zarejestruj IPC handlers:
  - `issueTracker:addConnection` — zapisz connection config + encrypted token
  - `issueTracker:removeConnection` — usun connection i token
  - `issueTracker:testConnection` — sprawdz czy credentials dzialaja
  - `issueTracker:getConnections` — lista aktywnych connections
  - `issueTracker:fetchBoards` — pobranie tablic z danego connection
  - `issueTracker:fetchStatuses` — pobranie statusow/kolumn
  - `issueTracker:fetchIssues` — pobranie taskow z filtrami
  - `issueTracker:getCurrentSprint` — aktywny sprint
- [ ] Dodaj metody do preload `api` object:
  - `issueTrackerAddConnection(config)`, `issueTrackerRemoveConnection(id)`, itd.
- [ ] Dodaj typy do `index.d.ts` w `CanopyAPI` interface

**Wzorce do zastosowania:**
```ts
// W handlers.ts — wzoruj sie na istniejacym git: pattern
ipcMain.handle('issueTracker:fetchIssues', async (_event, payload: {
  connectionId: string
  statuses: string[]
  assignedToMe: boolean
}) => {
  return issueTrackerManager.fetchIssues(payload)
})

// W preload/index.ts
issueTrackerFetchIssues: (connectionId: string, statuses: string[], assignedToMe: boolean) =>
  ipcRenderer.invoke('issueTracker:fetchIssues', { connectionId, statuses, assignedToMe }),
```

**Tech notes:**
- **Referencja:** wzoruj sie na `git:*` handlers w `handlers.ts`
- **Security:** Token nigdy nie przechodzi do renderera — manager pobiera go z encrypted store
- **Gotcha:** `ENCRYPTED_KEYS` set w PreferencesStore musi byc rozszerzony o nowe klucze
- **Walidacja:** IPC roundtrip dziala — renderer moze wywolac i dostac odpowiedz

---

### Faza 4: Branch template engine

**Cel:** Silnik renderowania nazw branchy z template + placeholderow

**Zalezy od:** Faza 1

**Lokalizacje:**
- `src/main/issueTracker/branchTemplate.ts` — parser i renderer template
- `src/renderer/src/lib/issueTracker/branchTemplate.ts` — ten sam kod (shared) lub wersja rendererowa do live preview

**Zadania:**
- [ ] Zdefiniuj dostepne placeholdery:
  - `{sprint}` — numer sprintu
  - `{sprintName}` — nazwa sprintu
  - `{issueKey}` — np. "GAKKO-21"
  - `{parentKey}` — klucz parent issue (story)
  - `{issueType}` — task/story/subtask/bug
  - `{issueTitle}` — tytul (slugified: lowercase, spaces→dashes, max 50 chars)
  - `{boardKey}` — prefix tablicy (np. "GAKKO")
  - Custom variables definiowane przez usera (stale, np. `{prefix}` = "feat")
- [ ] Implementuj `renderBranchName(template: string, variables: Record<string, string>, customVars: Record<string, string>): string`
  - Podmienia placeholdery na wartosci
  - Sanityzuje output (niedozwolone znaki w git branch names)
  - Obsluguje warunkowe segmenty: `{?parentKey}/{parentKey}` — segment renderowany tylko jesli parentKey istnieje
- [ ] Implementuj `validateTemplate(template: string): { valid: boolean; errors: string[] }`
- [ ] Implementuj `getAvailablePlaceholders(): PlaceholderInfo[]` — lista z opisami

**Wzorce do zastosowania:**
```ts
// Template przyklad: 's{sprint}/{issueKey}'
// Dla subtaska ze story: 's{sprint}/{parentKey}/{issueKey}'
// Dla story branch: 's{sprint}/{issueKey}/main'
// Z custom var: '{prefix}/s{sprint}/{issueKey}'

// Warunkowe segmenty:
// 's{sprint}/{?parentKey}{parentKey}/{?parentKey}{issueKey}'
// Jesli parentKey istnieje: s10/GAKKO-20/GAKKO-21
// Jesli nie: s10/GAKKO-21
```

**Tech notes:**
- **Gotcha:** Git branch names nie moga zawierac: spacji, ~, ^, :, ?, *, [, \, podwojnych kropek (..)
- **Edge cases:** Pusty placeholder (np. brak sprintu) — usun segment lub uzyj fallbacku
- **Performance:** Template rendering musi byc synchroniczny (uzywany w live preview)
- **Walidacja:** Zestaw testow z roznymi kombinacjami templateow i wartosci

---

### Faza 5: PR template engine

**Cel:** Silnik generowania tytulow i opisu PR + logika target branch

**Zalezy od:** Faza 1

**Lokalizacje:**
- `src/main/issueTracker/prTemplate.ts` — PR title/body renderer + target branch resolver

**Zadania:**
- [ ] Zdefiniuj PR template placeholdery:
  - `{issueKey}`, `{issueTitle}`, `{issueType}`, `{parentKey}`, `{boardKey}`
- [ ] Implementuj `renderPRTitle(template: string, issue: TrackerIssue): string`
  - Domyslny template: `'[{issueKey}] {issueTitle}'`
- [ ] Implementuj `renderPRBody(template: string, issue: TrackerIssue): string`
  - Moze zawierac link do issue: `{issueUrl}`
- [ ] Implementuj `resolveTargetBranch(issue: TrackerIssue, config: PRTargetConfig): string`
  - Reguly: jesli subtask → target = parent story branch (np. `s10/GAKKO-20/main`)
  - Jesli task/story → target = konfigurowana domyslna galaz (np. `develop`)
  - Konfigurowalne per-type mapping

**Wzorce do zastosowania:**
```ts
interface PRTargetConfig {
  defaultBranch: string // 'develop'
  rules: Array<{
    issueType: string // 'subtask'
    targetPattern: string // '{parentBranch}/main' lub konkretna nazwa
  }>
}
```

**Tech notes:**
- **Gotcha:** Target branch musi istniec — walidacja przed tworzeniem PR
- **Edge cases:** Story bez subtaskow, subtask bez story
- **Walidacja:** Poprawne renderowanie dla roznych typow issue

---

### Faza 6: Sidebar section — Issue Tracker

**Cel:** Nowa sekcja sidebar wyswietlajaca podlaczone trackery z przyciskami akcji

**Zalezy od:** Faza 3

**Lokalizacje:**
- `src/renderer/src/components/sidebar/IssueTrackerSection.svelte` — nowy komponent
- `src/renderer/src/lib/stores/sidebarSections.svelte.ts` — dodanie `'issues'` do SidebarSectionId
- `src/renderer/src/lib/stores/issueTracker.svelte.ts` — nowy store
- `src/renderer/src/components/sidebar/Sidebar.svelte` — rendering nowej sekcji

**Zadania:**
- [ ] Rozszerz `SidebarSectionId` o `'issues'`
- [ ] Dodaj `{ id: 'issues', visible: true }` do `DEFAULT_CONFIG`
- [ ] Dodaj definicje w `SECTION_DEFS`: `{ id: 'issues', label: 'Issues', forced: false }`
- [ ] Utworz `issueTracker.svelte.ts` store:
  - `connections: IssueTrackerConnection[]` — zaladowane z prefs
  - `activeIssues: Map<connectionId, TrackerIssue[]>` — pobrane taski
  - `loadConnections()`, `refreshIssues(connectionId)`, `selectIssue(issue)`
- [ ] Utworz `IssueTrackerSection.svelte`:
  - Lista polaczonych trackerow (ikona providera + nazwa)
  - Przy kazdym: przycisk "Browse Issues" otwierajacy dialog
  - Jesli brak connections — przycisk "Connect Tracker" → otwiera preferences
  - Status: loading/error/connected z odpowiednimi ikonami

**Wzorce do zastosowania:**
```svelte
<!-- Wzoruj sie na GitSection.svelte / ToolSection.svelte -->
<CollapsibleSection title="Issues" sectionKey="issues">
  {#if connections.length === 0}
    <button class="connect-btn" onclick={openTrackerPrefs}>
      Connect Issue Tracker
    </button>
  {:else}
    {#each connections as conn (conn.id)}
      <div class="tracker-row">
        <span class="tracker-icon">{providerIcon(conn.provider)}</span>
        <span class="tracker-name">{conn.name}</span>
        <button class="browse-btn" onclick={() => openIssuePicker(conn.id)}>
          Browse
        </button>
      </div>
    {/each}
  {/if}
</CollapsibleSection>
```

**Tech notes:**
- **Referencja:** `GitSection.svelte`, `ToolSection.svelte` — ten sam CollapsibleSection pattern
- **Gotcha:** Sekcja powinna renderowac sie nawet bez workspace (polaczenia sa globalne)
- **Walidacja:** Sekcja widoczna w sidebarze, reaguje na zmiany visibility w prefs

---

### Faza 7: Issue Picker Modal

**Cel:** Dialog do przegladania i wybierania taskow z trackera

**Zalezy od:** Faza 3, Faza 6

**Lokalizacje:**
- `src/renderer/src/components/issueTracker/IssuePickerModal.svelte` — nowy modal
- `src/renderer/src/lib/stores/dialogs.svelte.ts` — nowy typ dialogu

**Zadania:**
- [ ] Dodaj `IssuePickerState` do `DialogState` union type:
  ```ts
  | { type: 'issuePicker'; props: { connectionId: string; onSelect: (issue: TrackerIssue) => void } }
  ```
- [ ] Dodaj `showIssuePicker(connectionId, onSelect)` function
- [ ] Zaimplementuj `IssuePickerModal.svelte`:
  - Ladowanie: spinner podczas fetchowania issues
  - Wyszukiwarka na gorze (filtrowanie po kluczu i tytule)
  - Lista: `[GAKKO-21]  Title  |  status-badge  |  priority-icon`
  - Klik na issue → callback `onSelect` → zamkniecie modala
  - Board selector jesli connection ma wiele tablic
  - Keyboard: Escape zamyka, ArrowUp/Down nawigacja, Enter wybiera
- [ ] Zarejestruj modal w `App.svelte` (lub tam gdzie inne modale)

**Wzorce do zastosowania:**
```svelte
<!-- Styl z InputDialog.svelte -->
<div class="dialog-overlay" onkeydown={handleKeydown} onclick={close}>
  <div class="picker-container" onclick={(e) => e.stopPropagation()}>
    <div class="picker-header">
      <h3>Select Issue — {connectionName}</h3>
      <input class="search-input" bind:value={searchQuery} placeholder="Search issues..." />
    </div>
    <div class="issue-list">
      {#each filteredIssues as issue (issue.key)}
        <button class="issue-row" class:selected={issue.key === selectedKey}
          onclick={() => selectIssue(issue)}>
          <span class="issue-key">{issue.key}</span>
          <span class="issue-summary">{issue.summary}</span>
          <span class="status-badge">{issue.status}</span>
        </button>
      {/each}
    </div>
  </div>
</div>
```

**Tech notes:**
- **Referencja:** `InputDialog.svelte` (overlay + keyboard), `CreateWorktreeModal.svelte` (step machine, loading)
- **Gotcha:** Lista moze byc dluga — uzywaj virtualnej listy lub limit + "Load more"
- **Edge cases:** Brak issues (pusta tablica), blad API (error state z retry button)
- **Performance:** Debounce na search input (300ms)
- **Walidacja:** Modal otwiera sie, laduje issues, filtrowanie dziala, wybor zamyka modal

---

### Faza 8: Branch creation flow

**Cel:** Po wyborze issue → utworz branch z template → checkout (z obsluga dirty state)

**Zalezy od:** Faza 4, Faza 7

**Lokalizacje:**
- `src/renderer/src/lib/issueTracker/branchCreation.ts` — logika flow
- `src/main/issueTracker/branchCreation.ts` — server-side (git operations)
- Modyfikacja `IssuePickerModal.svelte` — po wyborze issue uruchom flow

**Zadania:**
- [ ] Implementuj flow po wybraniu issue:
  1. Pobierz aktualny sprint z API (jesli template uzywa `{sprint}`)
  2. Zrenderuj nazwe brancha z template
  3. Pokaz podglad nazwy brancha w confirm dialog
  4. Sprawdz czy repo jest dirty → jesli tak, pytaj: stash / commit / cancel
  5. Utworz branch: `git checkout -b <branchName>`
  6. Zaktualizuj workspace state
- [ ] Dodaj IPC handler `issueTracker:createBranch`:
  - Przyjmuje: repoRoot, branchName, baseBranch
  - Uzywa istniejacego `GitRepository` do operacji
- [ ] Dodaj handler `issueTracker:resolveBranchName`:
  - Przyjmuje: connectionId, issueKey
  - Zwraca: resolved branch name (server-side rendering template z danymi z API)

**Wzorce do zastosowania:**
```ts
// Flow: issue selected → resolve branch → confirm → create
async function handleIssueSelected(issue: TrackerIssue): Promise<void> {
  const branchName = await window.api.issueTrackerResolveBranchName(connectionId, issue.key)

  const confirmed = await confirm({
    title: 'Create Branch',
    message: `Create branch from selected issue?`,
    details: branchName,
    confirmLabel: 'Create & Checkout',
  })

  if (!confirmed) return

  // Check dirty state
  if (workspaceState.isDirty) {
    // ... stash/commit dialog
  }

  await window.api.issueTrackerCreateBranch(repoRoot, branchName, currentBranch)
}
```

**Tech notes:**
- **Referencja:** `CreateWorktreeModal.svelte` — podobny flow z tworzeniem brancha
- **Gotcha:** `git checkout -b` fail jesli branch juz istnieje — obsluz gracefully
- **Edge cases:** Brak aktywnego sprintu, issue bez parent key, special characters w tytule
- **Walidacja:** Caly flow od wyboru issue do checkout nowego brancha

---

### Faza 9: Task content injection do terminala

**Cel:** Mozliwosc wstawienia tresci taska jako prompt do aktywnej sesji terminala

**Zalezy od:** Faza 7

**Lokalizacje:**
- `src/renderer/src/components/issueTracker/IssuePickerModal.svelte` — przycisk "Send to Terminal"
- `src/renderer/src/lib/stores/tabs.svelte.ts` — znalezienie aktywnej sesji
- Preload: uzycie istniejacego `writePty` do wstawienia tekstu

**Zadania:**
- [ ] Dodaj przycisk "Send to Terminal" w IssuePickerModal (obok issue lub jako akcja po wyborze)
- [ ] Implementuj `sendIssueToTerminal(issue: TrackerIssue)`:
  - Sformatuj tresc: `"Issue: {key} - {summary}\n\n{description}"`
  - Znajdz aktywna sesje PTY (z tabs store)
  - Uzyj `window.api.writePty(sessionId, formattedText)` do wstawienia
- [ ] Alternatywnie: przycisk kontekstowy przy kazdym issue na liscie

**Wzorce do zastosowania:**
```ts
// Uzyj istniejacego writePty API
function sendToTerminal(issue: TrackerIssue): void {
  const activeSession = getActivePtySession() // z tabs store
  if (!activeSession) {
    toast('No active terminal session')
    return
  }
  const text = formatIssueForPrompt(issue)
  window.api.writePty(activeSession.sessionId, text)
}
```

**Tech notes:**
- **Referencja:** Patrz jak `writePty` jest uzywany w istniejacym kodzie
- **Gotcha:** Formatowanie Markdown moze nie dzialac w kazdym narzedziu — uzyj plain text z minimalnymi separatorami
- **Edge cases:** Brak aktywnej sesji terminala, bardzo dlugi opis taska (limit?)
- **Walidacja:** Tekst pojawia sie w aktywnym terminalu po kliknieciu

---

### Faza 10: Preferences panel — Issue Tracker

**Cel:** Zakladka w PreferencesModal do zarzadzania polaczeniami, szablonami i filtrami

**Zalezy od:** Faza 3, Faza 4, Faza 5

**Lokalizacje:**
- `src/renderer/src/components/preferences/IssueTrackerPrefs.svelte` — nowy panel
- `src/renderer/src/components/preferences/PreferencesModal.svelte` — dodanie zakladki

**Zadania:**
- [ ] Dodaj zakladke "Issues" w PreferencesModal (ikona: `SquareKanban` z Lucide)
- [ ] Sekcja "Connections":
  - Lista istniejacych polaczen (provider icon + name + status)
  - Przycisk "Add Connection" → formularz: provider (select), name, base URL, project key, username (Jira), API token (password input)
  - Przycisk "Test" przy kazdym connection
  - Przycisk "Remove" z confirm dialog
- [ ] Sekcja "Branch Naming":
  - Input z template string, np. `s{sprint}/{issueKey}`
  - Lista dostepnych placeholderow pod inputem
  - Custom variables: key-value pairs editor (przycisk + / -)
  - Live preview: wyswietl przykladowa nazwe brancha z mock data
- [ ] Sekcja "PR Naming":
  - Title template input, np. `[{issueKey}] {issueTitle}`
  - Default target branch input
  - Per-type target rules (np. subtask → `{parentBranch}/main`)
  - Live preview
- [ ] Sekcja "Task Filters":
  - Toggle: "Only assigned to me" vs "All matching tasks"
  - Lista statusow z checkboxami (pobrana z API po wyborze connection)
  - Przycisk "Refresh statuses" do ponownego pobrania
- [ ] Sekcja "Export / Import":
  - Przycisk "Export Configuration" → dialog save file (.json)
  - Przycisk "Import Configuration" → dialog open file (.json) + confirm overwrite

**Wzorce do zastosowania:**
```svelte
<!-- Wzoruj sie na GeneralPrefs.svelte / ClaudePrefs.svelte -->
<div class="section">
  <h3 class="section-title">Connections</h3>
  <p class="section-desc">Connect to issue tracking services.</p>
  <!-- ... -->
</div>
```

**Tech notes:**
- **Referencja:** `GeneralPrefs.svelte`, `ClaudePrefs.svelte` — ten sam styl i patterns
- **Gotcha:** API token musi byc maskowany w UI (type="password")
- **Security:** Token przesylany do main process via IPC, nigdy przechowywany w renderer state na dlugo
- **Edge cases:** Test connection fail, nieprawidlowy URL, brak uprawnien
- **Walidacja:** Kazda sekcja dziala niezaleznie, zmiany zapisuja sie natychmiast do prefs

---

### Faza 11: PR creation flow

**Cel:** Automatyczne tworzenie PR z poziomu Canopy

**Zalezy od:** Faza 5, Faza 8

**Lokalizacje:**
- `src/main/issueTracker/prCreation.ts` — logika tworzenia PR via git hosting API
- `src/main/ipc/handlers.ts` — nowy handler `issueTracker:createPR`
- `src/preload/index.ts` — nowa metoda API
- `src/renderer/src/components/issueTracker/CreatePRButton.svelte` — UI trigger (opcjonalny, moze byc w Git section)

**Zadania:**
- [ ] Implementuj PR creation:
  - Parsuj remote URL aby wykryc hosting (GitHub, GitLab, Bitbucket)
  - Uzyj odpowiedniego API do stworzenia PR
  - Dla GitHub: `gh` CLI lub GitHub REST API
  - Title i body z PR template engine
  - Target branch z PR target resolver
- [ ] Dodaj IPC handler `issueTracker:createPR`:
  - Przyjmuje: repoRoot, issueKey (do resolve template), sourceBranch
  - Zwraca: PR URL
- [ ] Dodaj przycisk w UI — np. w Git section sidebar lub jako akcja po ukonczeniu pracy na branchu

**Tech notes:**
- **Gotcha:** Wymaga push brancha przed utworzeniem PR — sprawdz i pushuj jesli trzeba
- **Gotcha:** `gh` CLI moze nie byc zainstalowany — fallback na REST API lub informacja o brakujacym narzedziu
- **Edge cases:** PR juz istnieje dla tego brancha, brak uprawnien do repo
- **Walidacja:** PR tworzony poprawnie, URL otwiera sie w przegladarce

---

### Faza 12: Export/Import konfiguracji

**Cel:** Pelny export i import ustawien issue trackera do pliku JSON

**Zalezy od:** Faza 10

**Lokalizacje:**
- `src/main/issueTracker/configExport.ts` — serializacja/deserializacja
- `src/main/ipc/handlers.ts` — handlery `issueTracker:exportConfig`, `issueTracker:importConfig`

**Zadania:**
- [ ] Implementuj `exportConfig(): IssueTrackerExportData`:
  - Zbierz: connections (BEZ tokenow!), branch template, PR template, filtry, custom vars
  - Serializuj do JSON
  - Electron `dialog.showSaveDialog` do wybrania sciezki
- [ ] Implementuj `importConfig(data: IssueTrackerExportData)`:
  - Waliduj schema (wersja, wymagane pola)
  - Nadpisz istniejaca konfiguracje (po potwierdzeniu usera)
  - Connections importowane BEZ tokenow — user musi je wpisac recznie
  - Electron `dialog.showOpenDialog` do wybrania pliku
- [ ] Format eksportu:
  ```json
  {
    "version": 1,
    "exportedAt": "2026-03-30T12:00:00Z",
    "connections": [{ "provider": "jira", "name": "...", "baseUrl": "...", "projectKey": "..." }],
    "branchTemplate": { "template": "s{sprint}/{issueKey}", "customVars": {} },
    "prTemplate": { "titleTemplate": "[{issueKey}] {issueTitle}", "targetRules": [] },
    "filters": { "assignedToMe": true, "statuses": ["To Do", "Open"] }
  }
  ```

**Tech notes:**
- **Security:** NIGDY nie eksportuj tokenow/haseł — to krytyczne
- **Gotcha:** Import starszej wersji — migracja schema (version field)
- **Walidacja:** Roundtrip: export → import → konfiguracja identyczna (oprocz credentials)

---

### Faza 13: Integracja i polish

**Cel:** Polaczenie wszystkich czesci, edge cases, UX polish

**Zalezy od:** Wszystkie poprzednie fazy

**Lokalizacje:** Wszystkie pliki z poprzednich faz

**Zadania:**
- [ ] Dodaj ikony Lucide: `SquareKanban` (sekcja), `CircleDot` (Jira), `Waypoints` (YouTrack), `GitBranch`, `GitPullRequest`
- [ ] Loading states wszedzie (spinners, skeleton loaders)
- [ ] Error handling: toast notifications dla bledow API, retry buttons
- [ ] Keyboard navigation w IssuePickerModal (arrows, enter, escape)
- [ ] Tooltip'y na przyciskach
- [ ] Responsywnosc modali (min/max width)
- [ ] Sprawdz ze nowa sekcja sidebar respektuje visibility/ordering z SidebarPrefs

**Tech notes:**
- **Walidacja:** Pelny flow end-to-end: connect → browse → select → branch → work → PR

---

## Weryfikacja koncowa

**Testy do wykonania:**
- [ ] `npm run typecheck` — zero bledow typow
- [ ] `npm run lint` — zero warningow
- [ ] `npm run svelte-check` — zero bledow
- [ ] Manual: Dodaj Jira connection → pobierz issues → wybierz task → branch created
- [ ] Manual: Dodaj YouTrack connection → ten sam flow
- [ ] Manual: Branch template z custom vars → poprawna nazwa
- [ ] Manual: PR creation → PR pojawia sie na GitHub/GitLab
- [ ] Manual: Export config → import na czystej instalacji → konfiguracja przywrocona
- [ ] Manual: Send issue to terminal → tekst pojawia sie w aktywnej sesji
- [ ] Manual: Sidebar section visibility/ordering → dziala z preferences

**Kryteria akceptacji:**
- [ ] Polaczenie z Jira Cloud dziala (auth, fetch issues, fetch sprints)
- [ ] Polaczenie z YouTrack dziala (auth, fetch issues, fetch sprints)
- [ ] Branch naming template z live preview i custom variables
- [ ] Automatyczne tworzenie brancha z checkout (z obsluga dirty state)
- [ ] PR creation z konfigurowalnym templatem i target branch
- [ ] Export/import konfiguracji (bez credentials)
- [ ] Wstrzykniecie tresci taska do aktywnego terminala
- [ ] Nowa sekcja sidebar z lista trackerow
- [ ] Preferences panel z pelna konfiguracja
- [ ] Wszystkie operacje API w main process (tokeny nie trafiaja do renderera)
