# Anonymous telemetry

> One daily HTTP ping to count active users, containing no personal data and no stable identifier.

**Status:** Stable
**Introduced:** v0.10.0
**Platforms:** All

## Overview

Canopy sends a single HTTP POST per day to a self-hosted Umami analytics instance at `analytics.itsol.tech`. The ping fires once when the app starts and again each time the main window regains focus, but only if a ping has not already been sent today.

The payload is intentionally minimal. It contains: screen resolution of the primary display, locale (from `app.getLocale()`), app version, OS platform (`darwin`, `linux`, `win32`), and CPU architecture (`arm64`, `x64`). No machine identifiers, no IP hashing on the server, no usage patterns, no file paths, no terminal content, no session duration.

Telemetry is enabled by default. Users can disable it during the onboarding wizard (Features step) or at any time in Settings > Privacy. Disabling it takes effect immediately; no restart is required.

## Behavior

### Daily ping

1. On app start, `TelemetryManager.init()` reads the stored `telemetry.lastPingDate` from preferences and caches it in memory.
2. `tryPing()` runs. If `telemetry.enabled` is `'false'`, it returns immediately.
3. If the cached date matches today's date (`YYYY-MM-DD`), no request is sent.
4. Otherwise, a single POST is sent to `https://analytics.itsol.tech/api/send` with a JSON body containing the Umami website ID, a fixed hostname (`app.canopy.itsol.tech`), the URL `/ping`, and the data fields listed above.
5. On success (HTTP 2xx), the date is persisted to `telemetry.lastPingDate`.
6. On failure (non-2xx or network error), both the in-memory cache and the stored date are cleared, so the next focus event retries.

### Window focus retry

1. Each time the main window receives focus, `TelemetryManager.onWindowFocus()` calls `tryPing()`.
2. If a ping already succeeded today, the cached date check short-circuits without any network request.
3. If a previous attempt failed (cache was cleared), the ping retries.

### Opt-out

1. The user unchecks "Minimal telemetry" in Settings > Privacy or during onboarding.
2. The preference `telemetry.enabled` is set to `'false'`.
3. All subsequent `tryPing()` calls return immediately without reading the date or making requests.
4. Re-enabling the checkbox sets the preference to `'true'` and the next focus or restart triggers a ping.

## Configuration

| Preference key           | Values               | Default            | Description                                                      |
| ------------------------ | -------------------- | ------------------ | ---------------------------------------------------------------- |
| `telemetry.enabled`      | `'true'` / `'false'` | `'true'` (enabled) | Master toggle. Checked at the start of every ping attempt.       |
| `telemetry.lastPingDate` | `'YYYY-MM-DD'`       | unset              | Stores the date of the last successful ping. Managed internally. |

## Exact payload

```json
{
  "type": "event",
  "payload": {
    "website": "e2ef58e3-bbc0-490c-9afb-263cbfce1640",
    "hostname": "app.canopy.itsol.tech",
    "url": "/ping",
    "language": "en-US", // varies by system locale (app.getLocale())
    "screen": "1920x1080", // primary display resolution
    "data": {
      "version": "0.11.0",
      "os": "darwin",
      "arch": "arm64"
    }
  }
}
```

The `User-Agent` header is set to Electron's default user agent string from `session.defaultSession.getUserAgent()`.

## Data NOT collected

The following are explicitly absent from the payload and from any other outbound request:

- Machine hostname, username, or home directory path
- Hardware serial numbers or MAC addresses
- Any stable device identifier (no UUID is generated or stored)
- Terminal command history or output
- File paths, project names, or repository URLs
- Feature usage frequency, session duration, or click events
- IP address hashing (the Umami instance is configured without IP tracking)

## Security and privacy

The telemetry endpoint uses HTTPS. The request is made through Electron's `net.fetch`, which respects system proxy settings. No cookies or tokens are attached.

Umami is a privacy-focused, open-source analytics platform. The instance at `analytics.itsol.tech` is self-hosted by the Canopy team. The privacy policy is linked from Settings > Privacy: `https://canopy.itsol.tech/privacy-policy`.

## Source files

- Main: `src/main/telemetry/TelemetryManager.ts`
- Preferences UI: `src/renderer/src/components/preferences/PrivacyPrefs.svelte`
- Onboarding step: `src/renderer/src/lib/onboarding/steps.ts` (id: `telemetry`)
