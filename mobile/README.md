# Canopy Remote

Native companion app for [Canopy](https://itsol.tech/canopy), using the unreleased
Rust host's native gRPC protocol over LAN, VPN or an app-owned pinned SSH tunnel.

Active routes use the local native module. Legacy WebRTC records remain preserved
but are not automatically migrated. This requires a new native binary, not an OTA
update. See [NATIVE-REMOTE.md](NATIVE-REMOTE.md) for implementation ownership,
verification evidence and unfinished release work. Native/GUI acceptance is NOT RUN.

## Prerequisites

- Node.js 22+
- [EAS CLI](https://docs.expo.dev/eas-update/getting-started/): `npm i -g eas-cli`
- Xcode 15+ (for iOS simulator / device builds)
- A running Canopy desktop instance with remote control enabled (see [protocol docs](../docs/features/remote-control.md))

## Setup

```bash
npm install
```

## Build profiles

| Profile             | Command                                                    | Output                         |
| ------------------- | ---------------------------------------------------------- | ------------------------------ |
| iOS simulator (dev) | `eas build --profile development-simulator --platform ios` | `.app` for Simulator           |
| iOS device (dev)    | `eas build --profile development --platform ios`           | `.ipa` for TestFlight internal |
| Production          | `eas build --profile production --platform ios`            | App Store `.ipa`               |

## Automatic EAS builds

`mobile/.eas/workflows/mobile-production.yml` starts cloud builds when a push to `next` or
`main` contains changes under `mobile/`:

- `next` builds a production iOS archive and a signed Android APK for internal testing.
- `main` builds a production iOS archive and an Android App Bundle for Google Play.

The workflow builds artifacts only; it does not submit them to TestFlight or Google Play. The
GitHub repository must be connected under the EAS project's **Settings → GitHub** for push
triggers to run.

## Dev against a local desktop

1. Use a Rust host build with the native remote service implemented and enabled; the Electron development server is not this protocol.
2. In Canopy: Preferences → Security → Remote Control → enable and select the reachable network adapter
3. Open the REMOTE section in the left sidebar and click Pair
4. Install the simulator build, tap Connect, scan the QR code
5. Accept on the desktop — the session is live

## EAS submit

Set the following env vars before submitting (or store them in EAS environment variables with `eas env:create`):

```
EXPO_APPLE_ID        # Apple ID email used for App Store Connect
EXPO_ASC_APP_ID      # Numeric App Store Connect App ID
EXPO_APPLE_TEAM_ID   # 10-character Apple Team ID
```

Then run:

```bash
eas submit --profile production --platform ios
```

## Protocol

See [NATIVE-REMOTE.md](NATIVE-REMOTE.md) and the [runtime plan](../docs/agent-runtime-remote-plan.md) for the native pairing, identity, lifecycle and acceptance contract. Historical WebRTC documentation does not describe the active native route.
