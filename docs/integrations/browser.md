# Built-in browser

> Render web pages in a sandboxed tab with DevTools, device emulation, credential autofill, and favorites management.

**Status:** Stable
**Introduced:** v0.10.0
**Platforms:** All

## Overview

Canopy includes a built-in browser tab that renders web content using Electron's `<webview>` tag backed by a `WebContentsView` for DevTools. Users can browse web pages, open an embedded DevTools panel, emulate mobile devices, autofill login credentials, capture screenshots, and manage a favorites list. Each browser tab runs in a shared persistent session partition (`persist:browser`) that is isolated from the main app's session.

The `BrowserManager` in the main process handles security enforcement (popup handling, navigation filtering, permission denial, keyboard shortcut interception) and DevTools lifecycle. The renderer tracks per-tab state (URL, title, favicon, loading, navigation, errors) in a reactive `browserSessions` record.

## Behavior

### Opening a browser tab

1. User creates a new browser tab or opens a URL from elsewhere in the app.
2. The renderer creates a `<webview>` element with `partition="persist:browser"` and calls `initBrowserSession(browserId)` to initialize the reactive state.
3. The renderer calls `setupBrowserWebview(browserId, webContentsId)` via IPC.
4. `BrowserManager.setup()` registers the webview's guest `WebContents` and attaches event listeners for popup forwarding, navigation filtering, keyboard interception, focus forwarding, favicon detection, and context menu.

### Navigation

1. User enters a URL in the address bar or clicks a link within the page.
2. The `will-navigate` handler blocks any navigation to non-HTTP(S) protocols (e.g., `javascript:`, `file:`, `data:`). Blocked navigations are silently prevented.
3. On successful navigation, the renderer updates `url`, `title`, `canGoBack`, `canGoForward`, and clears any previous error state.
4. Mouse back/forward buttons (app-command `browser-backward`/`browser-forward`) are handled at the window level and forwarded to the focused browser webview.
5. Links with `target="_blank"` (and `window.open()` calls) are intercepted by `setWindowOpenHandler`. Valid HTTP(S) URLs are forwarded to the renderer via `browser:openUrl`. The renderer honors the `urlOpenMode` preference: `canopy` opens a new browser pane tab in the same worktree via `openTool('browser', ...)`, `system` delegates to `shell.openExternal`, and `ask` shows a toast letting the user pick. Forwarding is throttled to one popup per 500ms per webview.

### Loading states and errors

1. When a page starts loading, `isLoading` is set to `true` and any existing error is cleared.
2. When loading completes, `isLoading` is set to `false`.
3. If loading fails, `handleBrowserLoadFailed` stores `{ code, description, url }` on the session. The error code is Chromium's `net::ERR_*` numeric code.

### Favicon detection

1. When a page's favicon changes, the `page-favicon-updated` event fires with one or more favicon URLs.
2. The main process fetches the first favicon URL using `session.fetch()` (bypassing CORS restrictions on the page's session).
3. The favicon is converted to a base64 data URL (`data:{contentType};base64,...`) and sent to the renderer via `browser:faviconChanged`.
4. If favicon fetch fails or no favicon is available, `null` is sent.

### Keyboard shortcut interception

The browser webview intercepts Cmd/Ctrl keyboard shortcuts to prevent them from being consumed by the web page:

- **Cmd+R**: Reloads the page within the webview (handled locally, not forwarded).
- **App shortcuts** (Cmd+W, Cmd+T, Cmd+K, Cmd+B, Cmd+D, Cmd+L, Cmd+O, Cmd+1-9, Cmd+Shift+I, Cmd+Shift+N, Cmd+comma): The webview consumes the event, refocuses the main renderer, and re-dispatches the keystroke so the app handles it.

### DevTools

1. User opens DevTools via the toolbar button, context menu "Inspect Element", or keyboard shortcut.
2. `BrowserManager.openDevTools()` creates a `WebContentsView` on first open and calls `setDevToolsWebContents()` to direct DevTools output into it. The view is added as a child of the window's content view.
3. The renderer controls DevTools bounds via `setBrowserDevToolsBounds(browserId, { x, y, width, height })`. The DevTools panel can be positioned at the bottom or left of the browser area (tracked as `devToolsMode`).
4. Closing DevTools hides the view by setting bounds to zero rather than destroying it. `setDevToolsWebContents()` can only be called once per `WebContents`, so the view is reused across open/close cycles.
5. "Inspect Element" opens DevTools and calls `wc.inspectElement(x, y)` at the context menu position.

### Device emulation

1. User selects a device preset from the toolbar (e.g., "iPhone 14 Pro", "Pixel 8").
2. The renderer calls `setBrowserDeviceEmulation(browserId, device)` with `{ width, height, scaleFactor, mobile }`.
3. `BrowserManager.setDeviceEmulation()` attaches the Chrome Debugger Protocol (version 1.3) and sends `Emulation.setDeviceMetricsOverride`.
4. To clear emulation, the renderer passes `null`. The manager sends `Emulation.clearDeviceMetricsOverride` and detaches the debugger.

Built-in viewport presets:

| Device             | Width | Height | Scale  | Mobile |
| ------------------ | ----- | ------ | ------ | ------ |
| iPhone SE          | 375   | 667    | 2x     | yes    |
| iPhone 14 Pro      | 393   | 852    | 3x     | yes    |
| iPhone 14 Pro Max  | 430   | 932    | 3x     | yes    |
| iPad Mini          | 768   | 1024   | 2x     | yes    |
| iPad Pro 11"       | 834   | 1194   | 2x     | yes    |
| Samsung Galaxy S24 | 360   | 780    | 3x     | yes    |
| Pixel 8            | 412   | 915    | 2.625x | yes    |

Users can add custom viewport presets stored in the `viewports.custom` preference key.

### Credential autofill

1. User triggers autofill for a stored credential on the current page.
2. The renderer calls `getCredentialDecrypted(id, domain, 'autofill')`. On the first autofill of the session the main process prompts the OS for authentication (Touch ID on macOS, `UserConsentVerifier` on Windows, a confirmation dialog on Linux). After a successful prompt the session is flagged as authenticated and subsequent autofills within the same app session skip both the OS prompt and `safeStorage.decryptString()` — matching Chrome's autofill behavior. The flag and the in-memory decrypted credential cache live only in the main process and are cleared on app quit or on any credential save/delete/import. The `'reveal'` purpose used by Settings → Saved Passwords always re-authenticates and never consults the cache, so revealing a plaintext password in the UI always requires a fresh OS prompt — matching Chrome's `chrome://password-manager`.
3. With the decrypted credential the renderer calls `fillBrowserCredential(browserId, username, password)`.
4. `BrowserManager.fillCredential()` executes JavaScript in an isolated world (ID 999) that:
   - Finds the first `<input type="password">` on the page.
   - Locates the nearest username field within the same form (by type `email`/`text` or name attributes containing `user`/`email`/`login`, or `autocomplete="username"`).
   - Sets values on both fields and dispatches `input` and `change` events with `bubbles: true` so frameworks detect the change.
5. The isolated world prevents page scripts from intercepting the injected values.

### Screenshot capture

1. User triggers a screenshot capture.
2. The renderer calls `capturePage()` on the webview element to get a PNG buffer.
3. The buffer is sent to `saveBrowserCapture(buffer)` which writes it to a temp file (`canopy-capture-{uuid}.png` in `os.tmpdir()`) and returns the file path.

### Context menu

Right-clicking in the browser shows a native context menu with: Back, Forward, Reload, Copy, Paste, and Inspect Element. Back/Forward are disabled when navigation history does not allow them. Copy/Paste respect the page's current edit state.

### Favorites

Users can bookmark pages as favorites. Favorites are stored in the `browser.favorites` preference key as a JSON array of `{ url, name, favicon }` objects. Operations:

- `addFavorite(fav)`: Adds or replaces a favorite by URL.
- `removeFavorite(url)`: Removes a single favorite by exact URL.
- `removeFavoritesByHost(url)`: Removes all favorites matching the URL's host.
- `updateFavorite(oldUrl, updated)`: Replaces a favorite entry.
- `reorderFavorites(fromIndex, toIndex)`: Moves a favorite to a new position in the list.
- `isFavorite(url)`: Checks if any favorite shares the same host as the given URL.

### Background throttling

The renderer can control whether a browser tab's web contents are throttled when backgrounded via `setBrowserBackgroundThrottling(browserId, allowed)`. When `allowed` is `false`, the page continues running timers and animations at full speed even when the tab is not visible.

### Teardown

When a browser tab is closed, the renderer calls `teardownBrowserWebview(browserId)`. The main process closes DevTools, detaches the debugger, removes the DevTools view from the window, and deletes the entry. `teardownAllForWindow(win)` cleans up all browser entries when a window is destroyed.

## Configuration

| Preference key      | Type        | Purpose                                                          |
| ------------------- | ----------- | ---------------------------------------------------------------- |
| `viewports.custom`  | JSON string | Custom device emulation presets (same shape as built-in presets) |
| `browser.favorites` | JSON string | Array of `{ url, name, favicon }` bookmark objects               |

## Error states

| Error                  | User sees                                              | Cause                                                                |
| ---------------------- | ------------------------------------------------------ | -------------------------------------------------------------------- |
| Page load failure      | Error overlay with Chromium error code and description | DNS failure, connection refused, SSL error, timeout                  |
| Blocked navigation     | Nothing (silently prevented)                           | Page attempted navigation to non-HTTP(S) protocol                    |
| Throttled popup        | Nothing (silently denied)                              | Multiple `window.open()` / `target="_blank"` within 500ms            |
| Blocked permission     | Nothing (silently denied)                              | Page requested camera, microphone, geolocation, or other permissions |
| DevTools view creation | DevTools button has no effect                          | `WebContents` for the webview guest could not be found               |
| Favicon fetch failure  | Tab shows no favicon                                   | CORS error, network error, or invalid favicon URL                    |

## Security and privacy

- The browser runs in a dedicated Electron session partition (`persist:browser`), isolated from the app's main session. Cookies, storage, and cache are separate.
- All permission requests (camera, microphone, geolocation, notifications, etc.) are denied via `setPermissionRequestHandler`.
- Popups (`window.open()`, `target="_blank"`) are denied at the Electron level via `setWindowOpenHandler`. Valid HTTP(S) URLs are instead forwarded to the renderer, which opens them as a new browser pane tab in the same worktree. The `<webview>` has `allowpopups` set so the handler is actually invoked. Forwarding is throttled to one popup per 500ms per webview to prevent flooding from malicious pages.
- Navigation is restricted to `http:` and `https:` protocols.
- Credential autofill runs in isolated JavaScript world 999 to prevent page scripts from observing the injected values.
- The Chrome Debugger Protocol is attached only when device emulation is active and detached when emulation is cleared.

## Source files

- Main: `src/main/browser/BrowserManager.ts`
- Store: `src/renderer/src/lib/browser/browserState.svelte.ts`
