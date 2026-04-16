# Canopy Remote

iOS companion app for [Canopy](https://itsol.tech/canopy) — control your desktop terminal from your phone over local Wi-Fi. Connects via WebRTC data channels after a one-time QR code pair.

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

## Dev against a local desktop

1. Start the desktop app: `npm run dev` from the repo root
2. In Canopy: Preferences → Security → Remote Control → enable
3. Open pairing modal: `Cmd+Shift+P` → "Open Remote Connection"
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

See [docs/features/remote-control.md](../docs/features/remote-control.md) for the full pairing flow, WebRTC architecture, trust model, and listen mode behaviour.
