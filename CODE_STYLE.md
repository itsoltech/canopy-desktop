# Writing efficient Electron + Svelte/TypeScript code - code review checklist

## Electron process architecture

- separate code into three layers: main process, preload, renderer
- main process (`src/main/`) handles: windows, IPC handlers, filesystem, native APIs, tray, auto-updater
- preload (`src/preload/`) exposes API to the renderer through `contextBridge`
- renderer (`src/renderer/`) is the Svelte app - no access to Node.js APIs
- never import `electron`, `fs`, `path`, `child_process` in the renderer
- one file = one responsibility: don't mix window management with IPC handlers

```
src/
├── main/
│   ├── index.ts            # entry point, app lifecycle
│   ├── windows.ts          # BrowserWindow creation and management
│   ├── ipc/                # IPC handlers grouped by feature
│   │   ├── files.ts
│   │   ├── sessions.ts
│   │   └── settings.ts
│   ├── services/           # main process business logic
│   ├── menu.ts             # application menu
│   └── updater.ts          # auto-updater
├── preload/
│   ├── index.ts            # contextBridge.exposeInMainWorld
│   └── api.ts              # API type definitions
├── renderer/
│   ├── App.svelte
│   ├── lib/
│   │   ├── api.ts          # typed wrapper over window.api
│   │   ├── stores/         # Svelte stores
│   │   └── utils/
│   ├── components/
│   │   ├── ui/             # primitive components
│   │   └── [feature]/      # feature-specific
│   └── routes/             # if using SvelteKit or custom router
└── shared/
    └── types.ts            # types shared between main/preload/renderer
```

## Electron security

- **always** set `contextIsolation: true` and `nodeIntegration: false` in `BrowserWindow`
- **always** set `sandbox: true` unless you have a documented reason not to
- preload script exposes a minimal API through `contextBridge.exposeInMainWorld`
- don't expose generic methods like `invoke(channel, ...args)` - each operation gets a dedicated function
- validate all data coming from the renderer in the main process (renderer = untrusted)
- don't pass `event.sender` to downstream functions - extract the data you need and work with copies
- `webPreferences.webSecurity` always `true` (default)
- restrict `webContents.setWindowOpenHandler` - don't allow opening arbitrary URLs

```typescript
// Good - preload with minimal, typed API
// preload/index.ts
import { contextBridge, ipcRenderer } from "electron";

const api = {
  getSettings: () => ipcRenderer.invoke("settings:get"),
  saveSettings: (settings: AppSettings) =>
    ipcRenderer.invoke("settings:save", settings),
  onSessionOutput: (callback: (data: string) => void) => {
    const handler = (_event: IpcRendererEvent, data: string) => callback(data);
    ipcRenderer.on("session:output", handler);
    return () => ipcRenderer.removeListener("session:output", handler);
  },
} as const;

contextBridge.exposeInMainWorld("api", api);

// Bad - generic invoke exposes any channel
contextBridge.exposeInMainWorld("electron", {
  invoke: (channel: string, ...args: unknown[]) =>
    ipcRenderer.invoke(channel, ...args),
});
```

## IPC - inter-process communication

- **no `ipcRenderer.send`/`ipcMain.on` for request-response** - use `ipcRenderer.invoke` / `ipcMain.handle`
- `send`/`on` only for fire-and-forget events (e.g. main > renderer notifications)
- name IPC channels using `feature:action` convention (e.g. `files:read`, `session:start`)
- define channel types in `shared/types.ts` - both payload and return type
- every `ipcMain.handle` validates its arguments before executing
- don't return objects with circular references or class instances - IPC serializes via structured clone
- long operations (>100ms) should run in a hidden `BrowserWindow` or `utilityProcess`

```typescript
// Good - typed IPC with validation
// shared/types.ts
export interface IpcChannels {
  "files:read": { args: [filePath: string]; return: string };
  "files:write": { args: [filePath: string, content: string]; return: void };
  "session:start": { args: [config: SessionConfig]; return: SessionId };
}

// main/ipc/files.ts
import { ipcMain } from "electron";
import { existsSync } from "node:fs";
import { readFile, writeFile } from "node:fs/promises";

export function registerFileHandlers() {
  ipcMain.handle("files:read", async (_event, filePath: string) => {
    if (typeof filePath !== "string" || !existsSync(filePath)) {
      throw new Error(`Invalid path: ${filePath}`);
    }
    return readFile(filePath, "utf-8");
  });
}

// Bad - no validation, send instead of invoke
ipcMain.on("readFile", (event, path) => {
  const content = readFileSync(path, "utf-8"); // no validation, sync I/O
  event.reply("fileContent", content);
});
```

## Svelte components

- use Svelte 5 with runes (`$state`, `$derived`, `$effect`, `$props`)
- one exported component per file
- component files in PascalCase: `TaskCard.svelte`
- **file length limits**:
  - components: 150-300 lines
  - pages/layouts: max 500 lines
  - `.svelte.ts` modules: max 200 lines
- `<script lang="ts">` mandatory - no plain JS
- `<script>` block on top, then markup, then `<style>` at the bottom
- avoid `<script context="module">` in Svelte 5 - use separate `.ts` modules

```svelte
<!-- Good - Svelte 5 with runes -->
<script lang="ts">
  import type { Task } from "$lib/types";

  interface Props {
    task: Task;
    onSelect: (id: string) => void;
    isSelected?: boolean;
  }

  let { task, onSelect, isSelected = false }: Props = $props();

  let isHovered = $state(false);
  let displayName = $derived(task.title || "Untitled");
</script>

<button
  class="task-card"
  class:selected={isSelected}
  onclick={() => onSelect(task.id)}
  onmouseenter={() => (isHovered = true)}
  onmouseleave={() => (isHovered = false)}
>
  <span>{displayName}</span>
</button>

<!-- Bad - legacy style without runes -->
<script lang="ts">
  export let task: Task;
  export let onSelect: (id: string) => void;
  export let isSelected = false;

  $: displayName = task.title || "Untitled";
</script>
```

## Props and typing

- define props via `interface Props` + `$props()`
- **max 5-7 props** per component - beyond that, split into smaller components or use Context
- default values in `$props()` destructuring
- type callback props as `(param: Type) => void`
- avoid `$$restProps` - explicitly declare what the component accepts
- replace slot props with snippets (Svelte 5)

```svelte
<!-- Good - explicit props with defaults -->
<script lang="ts">
  interface Props {
    title: string;
    variant?: "default" | "destructive" | "outline";
    disabled?: boolean;
    onclick?: (e: MouseEvent) => void;
  }

  let { title, variant = "default", disabled = false, onclick }: Props = $props();
</script>

<!-- Bad - $$restProps and no typing -->
<script lang="ts">
  export let title: string;
</script>
<button {...$$restProps}>{title}</button>
```

## TypeScript strictness

- **`any` is banned** - every usage requires `// eslint-disable` + a justification comment
- **`as` without a comment is banned** - every type assertion requires an explanation
- prefer type guards over assertions
- use `unknown` instead of `any` for IPC data and external sources
- `strict: true` in `tsconfig.json` is mandatory
- `noUnusedLocals: true`, `noUnusedParameters: true`

```typescript
// Good - type guard for IPC data
function isSessionOutput(data: unknown): data is SessionOutput {
  return (
    typeof data === "object" &&
    data !== null &&
    "sessionId" in data &&
    "content" in data
  );
}

// Good - justified assertion
const target = e.target as HTMLInputElement; // handler is on <input>, target is always HTMLInputElement

// Bad
const data: any = await window.api.getSettings();
```

## State management

- **`$state`** = local component state (UI toggles, form values)
- **`$derived`** = values computed from other state
- **Svelte stores** (`writable`, `readable`) = shared state across components
- **`.svelte.ts` modules** = reactive state outside components (Svelte 5 universal reactivity)
- don't mix responsibilities: keep server data stores separate from UI stores

```typescript
// Good - store with clear responsibility
// lib/stores/ui.svelte.ts
class UIState {
  sidebarOpen = $state(true);
  activePanel = $state<"files" | "terminal" | "settings">("files");

  toggleSidebar() {
    this.sidebarOpen = !this.sidebarOpen;
  }
}

export const uiState = new UIState();

// Good - classic writable store
// lib/stores/sessions.ts
import { writable, derived } from "svelte/store";
import type { Session } from "$lib/types";

export const sessions = writable<Session[]>([]);
export const activeSessionId = writable<string | null>(null);

export const activeSession = derived(
  [sessions, activeSessionId],
  ([$sessions, $id]) => $sessions.find((s) => s.id === $id) ?? null,
);

// Bad - server data and UI state in a single store
export const appStore = writable({
  sessions: [],        // server data
  sidebarOpen: true,   // UI state
  theme: "dark",       // configuration
});
```

## Communicating with main process from renderer

- typed wrapper over `window.api` in `lib/api.ts`
- wrap async operations in functions with error handling
- IPC event listeners require cleanup in `$effect` or `onDestroy`
- don't call IPC in loops - batch operations on the main process side

```svelte
<script lang="ts">
  import { api } from "$lib/api";
  import { onDestroy } from "svelte";

  let output = $state("");

  // Good - listener cleanup
  const unsubscribe = api.onSessionOutput((data) => {
    output += data;
  });

  onDestroy(() => {
    unsubscribe();
  });

  // Good - async with error handling
  async function startSession() {
    try {
      await api.startSession({ cwd: "/home/user/project" });
    } catch (err) {
      console.error("Failed to start session:", err);
      // show toast / error state
    }
  }
</script>
```

```typescript
// lib/api.ts - typed wrapper
// Good
interface ElectronApi {
  getSettings: () => Promise<AppSettings>;
  saveSettings: (settings: AppSettings) => Promise<void>;
  startSession: (config: SessionConfig) => Promise<string>;
  onSessionOutput: (callback: (data: string) => void) => () => void;
}

function getApi(): ElectronApi {
  if (!window.api) {
    throw new Error("Electron API not available - running outside Electron?");
  }
  return window.api as ElectronApi; // preload exposes matching shape
}

export const api = getApi();
```

## Effects and lifecycle

- `$effect` in Svelte 5 handles side effects (subscriptions, DOM sync, timers)
- `$effect` automatically tracks dependencies - don't specify them manually
- return a cleanup function from `$effect`
- don't use `$effect` for data transformations - use `$derived` for that
- `onMount` is still valid for one-time initialization without reactivity
- `onDestroy` for cleanup that doesn't depend on reactive state

```svelte
<script lang="ts">
  // Good - $effect with cleanup
  let sessionId = $state<string | null>(null);

  $effect(() => {
    if (!sessionId) return;

    const unsubscribe = api.onSessionOutput(sessionId, (data) => {
      appendOutput(data);
    });

    return () => unsubscribe();
  });

  // Good - $derived instead of $effect for transformations
  let filteredItems = $derived(items.filter((i) => i.status === "active"));

  // Bad - $effect for data transformation
  let filtered = $state<Item[]>([]);
  $effect(() => {
    filtered = items.filter((i) => i.status === "active");
  });

  // Bad - no cleanup for timer
  $effect(() => {
    setInterval(() => tick(), 1000);
  });
</script>
```

## Event handling

- use `onclick`, `oninput`, `onkeydown` (Svelte 5 syntax, no colon)
- don't do async directly in event handlers - delegate to a separate async function
- inline event handlers for simple operations, extract to functions for logic >1 line
- `e.preventDefault()` and `e.stopPropagation()` explicitly when needed

```svelte
<!-- Good - simple inline, complex in function -->
<button onclick={() => (isOpen = !isOpen)}>Toggle</button>

<button onclick={handleSave}>Save</button>

<script lang="ts">
  function handleSave() {
    saveMutation(formData);
  }
</script>

<!-- Bad - async inline -->
<button onclick={async () => {
  await api.saveSettings(settings);
  showToast("Saved");
}}>Save</button>
```

## Styling

- **scoped CSS** in the Svelte `<style>` block (default behavior)
- Tailwind CSS as an alternative - then skip `<style>` blocks
- don't mix Tailwind with custom `<style>` in the same component
- max 10-12 Tailwind classes per element - beyond that, extract to a component or use `@apply` in global CSS
- avoid `!important`
- CSS custom properties (variables) for theme-dependent values
- `class:name={condition}` directive for conditional classes (when not using Tailwind)

```svelte
<!-- Good - scoped CSS -->
<div class="container" class:active={isActive}>
  <span class="label">{title}</span>
</div>

<style>
  .container {
    display: flex;
    padding: 0.5rem;
    border-radius: var(--radius-md);
    background: var(--color-surface);
  }
  .container.active {
    background: var(--color-surface-active);
  }
</style>

<!-- Good - Tailwind -->
<div class="flex p-2 rounded-md bg-surface {isActive ? 'bg-surface-active' : ''}">
  <span class="text-sm font-medium">{title}</span>
</div>

<!-- Bad - mixing approaches -->
<div class="flex p-2" class:active={isActive}>
  <span>{title}</span>
</div>
<style>
  .active { background: blue; }
</style>
```

## Error handling

- wrap IPC calls in try/catch - IPC can throw on missing response, timeout, or main process crash
- error state in components: handle loading, success, and error
- global `unhandledrejection` and `error` handlers in the renderer - forward logs to main process
- main process: `process.on('uncaughtException')` - log and shut down gracefully

```svelte
<script lang="ts">
  let error = $state<string | null>(null);
  let loading = $state(false);
  let data = $state<Settings | null>(null);

  async function loadSettings() {
    loading = true;
    error = null;
    try {
      data = await api.getSettings();
    } catch (err) {
      error = err instanceof Error ? err.message : "Unknown error";
    } finally {
      loading = false;
    }
  }
</script>

{#if loading}
  <Spinner />
{:else if error}
  <Alert variant="destructive">{error}</Alert>
{:else if data}
  <SettingsForm settings={data} />
{/if}
```

## Performance

- avoid large lists without virtualization - above ~200 elements, use a virtual list
- IPC is expensive - don't call it in loops, batch data on the main process side
- `{#key expression}` forces full component recreation - use sparingly
- `$effect` doesn't require manual optimization, but avoid heavy operations inside it
- pass images and large data via file paths, not IPC payloads
- for lists with many elements, use `{#each items as item (item.id)}` with a key

```svelte
<!-- Good - keyed each -->
{#each sessions as session (session.id)}
  <SessionTab {session} />
{/each}

<!-- Bad - no key, Svelte can't diff efficiently -->
{#each sessions as session}
  <SessionTab {session} />
{/each}
```

```typescript
// Good - batch IPC
// main process
ipcMain.handle("files:readMultiple", async (_event, paths: string[]) => {
  return Promise.all(paths.map((p) => readFile(p, "utf-8")));
});

// Bad - N IPC calls in a loop
for (const path of paths) {
  const content = await window.api.readFile(path);
  results.push(content);
}
```

## Electron window management

- keep window references in a Map or dedicated module - not in global variables
- listen for the `closed` event and remove references (prevents memory leaks)
- call `BrowserWindow.loadFile` / `loadURL` after window configuration, not before
- for multiple windows: each window gets its own preload if it needs a different API
- use `webContents.setWindowOpenHandler` instead of `window.open` from the renderer

```typescript
// Good - window management
// main/windows.ts
const windows = new Map<string, BrowserWindow>();

export function createMainWindow(): BrowserWindow {
  const win = new BrowserWindow({
    width: 1200,
    height: 800,
    webPreferences: {
      preload: join(__dirname, "../preload/index.js"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });

  windows.set("main", win);

  win.on("closed", () => {
    windows.delete("main");
  });

  win.loadFile(join(__dirname, "../renderer/index.html"));
  return win;
}

export function getWindow(id: string): BrowserWindow | undefined {
  return windows.get(id);
}
```

## Auto-updater

- use `electron-updater` (from `electron-builder`) or Electron's built-in autoUpdater
- check for updates after app start with a delay (5-10s)
- don't force restart - inform the user and let them choose
- log updater events to a file (main process)
- disable auto-update in dev mode

## Testing

- **required tests for**:
  - IPC handlers (unit tests in main process)
  - `.svelte.ts` state modules
  - utility functions in `lib/`
  - validation of data entering/leaving IPC
- **optional**: Svelte components (snapshot/interaction tests)
- framework: Vitest
- E2E: Playwright + Electron (if needed)
- mock `window.api` in renderer tests
- test main process handlers without Electron runtime (isolate logic from ipcMain)

```typescript
// Good - isolating logic from IPC
// main/services/settings.ts
export async function loadSettings(configPath: string): Promise<AppSettings> {
  const raw = await readFile(configPath, "utf-8");
  return JSON.parse(raw) as AppSettings;
}

// main/ipc/settings.ts
ipcMain.handle("settings:get", () => loadSettings(CONFIG_PATH));

// test - testing loadSettings without ipcMain
describe("loadSettings", () => {
  it("parses valid config", async () => {
    const settings = await loadSettings("./fixtures/config.json");
    expect(settings.theme).toBe("dark");
  });
});
```

## Naming conventions

| Type | Convention | Example |
|------|-----------|---------|
| Component (file) | PascalCase | `TaskCard.svelte` |
| Reactive module | kebab-case + `.svelte.ts` | `session-state.svelte.ts` |
| Store (file) | kebab-case | `sessions.ts` |
| IPC handler (file) | kebab-case | `file-handlers.ts` |
| IPC channel | `feature:action` | `session:start` |
| Utility (file) | kebab-case | `format-bytes.ts` |
| Test (file) | kebab-case | `settings.test.ts` |
| Shared types | PascalCase in file | `SessionConfig`, `AppSettings` |

## Imports

- path alias: `$lib/` > `src/renderer/lib/` (SvelteKit convention)
- avoid barrel files - use direct imports
- type imports via `import type { ... }` (don't mix with runtime imports)
- ordering:
  1. Svelte (`svelte`, `svelte/store`)
  2. External libraries
  3. `$lib/` imports
  4. Relative imports

```svelte
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import type { Session } from "$lib/types";
  import { api } from "$lib/api";
  import { sessions } from "$lib/stores/sessions";
  import SessionTab from "./SessionTab.svelte";
</script>
```

## Build and packaging

- electron-builder or electron-forge for building installers
- separate `dependencies` (main process) from `devDependencies` (renderer, build tools)
- Vite as the renderer bundler (`vite-plugin-electron` or `electron-vite`)
- don't bundle native Node.js modules - leave them as external
- `asar: true` (default) - don't disable without a reason
- test the built application before release (not just dev mode)
- release profile:
  - renderer minification
  - source maps separate (don't pack into asar)
  - disable DevTools in production (`ELECTRON_DISABLE_SECURITY_WARNINGS` is not sufficient)

## Logging

- main process: dedicated logger (`electron-log`, `pino`, or custom) - not `console.log`
- renderer: `console.*` in dev, forward logs to main via IPC in production
- log: session starts/stops, IPC errors, crash recovery, auto-update events
- don't log user data (file paths are OK, file contents are not)
- log rotation - don't let files grow without a limit

## Accessibility

- keyboard navigation for interactive elements
- `aria-label` for icon buttons
- semantic HTML: `<button>`, `<nav>`, `<main>`, `<dialog>` instead of `<div>`
- focus management: focus trap in modals/dialogs
- respect `prefers-reduced-motion` and `prefers-color-scheme`

```svelte
<!-- Good -->
<button aria-label="Close session" onclick={() => closeSession(id)}>
  <CloseIcon />
</button>

<!-- Bad -->
<div class="clickable" onclick={() => closeSession(id)}>
  <CloseIcon />
</div>
```

## Code review checklist

- [ ] `contextIsolation: true`, `nodeIntegration: false`, `sandbox: true`
- [ ] Preload exposes minimal, typed API (no generic invoke)
- [ ] Data from renderer validated in main process
- [ ] IPC request-response via `invoke`/`handle` (not `send`/`on`)
- [ ] IPC event listeners have cleanup in `$effect`/`onDestroy`
- [ ] Component is <=300 lines (page <=500)
- [ ] Max 7 props, composition beyond that
- [ ] No `any` without a justification comment
- [ ] No `as` without a comment
- [ ] `$derived` for data transformations (not `$effect`)
- [ ] Tests for IPC handlers and utility functions
- [ ] Keyed `{#each}` with unique identifiers
- [ ] `strict: true` in `tsconfig.json`
- [ ] Build passes without warnings
