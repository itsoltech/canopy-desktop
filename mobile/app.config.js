const VARIANT = process.env.NODE_ENV || 'development'

const variantConfig = require(`./app.config.${VARIANT}.js`)

const base = {
  expo: {
    name: 'Canopy Remote',
    slug: 'canopy-remote',
    version: '1.0.0',
    orientation: 'portrait',
    icon: './assets/images/icon.png',
    scheme: 'canopy-remote',
    userInterfaceStyle: 'automatic',
    owner: 'itsol',
    runtimeVersion: {
      policy: 'appVersion',
    },
    updates: {
      checkAutomatically: 'NEVER',
    },
    ios: {
      icon: {
        light: './assets/images/icon-ios-light.png',
        dark: './assets/images/icon-ios-dark.png',
        tinted: './assets/images/icon-ios-tinted.png',
      },
      bundleIdentifier: 'tech.itsol.canopy',
      supportsTablet: true,
      infoPlist: {
        ITSAppUsesNonExemptEncryption: false,
      },
    },
    android: {
      package: 'tech.itsol.canopy',
      adaptiveIcon: {
        backgroundColor: '#FFFFFF',
        foregroundImage: './assets/images/android-icon-foreground.png',
        backgroundImage: './assets/images/android-icon-background.png',
        monochromeImage: './assets/images/android-icon-monochrome.png',
      },
      predictiveBackGestureEnabled: false,
      // react-native-webrtc's config plugin unconditionally adds audio/video
      // permissions because most consumers use getUserMedia for calls. Mobile
      // only opens WebRTC *data channels* (PTY + control flow), never touches
      // the camera or mic, so strip those permissions from the final manifest.
      blockedPermissions: [
        'android.permission.CAMERA',
        'android.permission.RECORD_AUDIO',
        'android.permission.MODIFY_AUDIO_SETTINGS',
        'android.permission.BLUETOOTH',
        'android.permission.SYSTEM_ALERT_WINDOW',
      ],
    },
    web: {
      output: 'static',
      favicon: './assets/images/favicon.png',
    },
    plugins: [
      'expo-router',
      [
        'expo-splash-screen',
        {
          backgroundColor: '#FFFFFF',
          image: './assets/images/splash-icon.png',
          imageWidth: 200,
          android: {
            image: './assets/images/splash-icon.png',
            imageWidth: 200,
          },
        },
      ],
      [
        'expo-dev-client',
        {
          launchMode: 'most-recent',
        },
      ],
      [
        'expo-camera',
        {
          cameraPermission: 'Scan QR codes from your Canopy desktop.',
          microphonePermission: false,
          recordAudioAndroid: false,
        },
      ],
      'expo-secure-store',
      '@config-plugins/react-native-webrtc',
      // Must run after @config-plugins/react-native-webrtc so it can strip
      // the NSCameraUsageDescription / NSMicrophoneUsageDescription keys
      // the plugin unconditionally writes. We use data channels only.
      './plugins/strip-webrtc-ios-permissions.js',
    ],
    experiments: {
      typedRoutes: true,
      reactCompiler: true,
    },
    extra: {
      router: {},
      eas: {
        projectId: 'b50d0f97-2243-4c45-bd3d-d7f4d132a1d9',
      },
    },
  },
}

function deepMerge(a, b) {
  if (b === undefined || b === null) return a
  if (Array.isArray(a) || Array.isArray(b)) return b
  if (typeof a !== 'object' || typeof b !== 'object' || a === null) return b
  const out = { ...a }
  for (const key of Object.keys(b)) {
    out[key] = key in a ? deepMerge(a[key], b[key]) : b[key]
  }
  return out
}

module.exports = deepMerge(base, variantConfig)
