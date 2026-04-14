import { CameraView, useCameraPermissions } from 'expo-camera'
import { useRouter } from 'expo-router'
import { useRef, useState } from 'react'
import { Alert, Platform, Pressable, StyleSheet, View } from 'react-native'
import { SafeAreaView } from 'react-native-safe-area-context'

import { ThemedText } from '@/components/themed-text'
import { ThemedView } from '@/components/themed-view'
import { Spacing } from '@/constants/theme'
import { parsePairingUrl } from '@/lib/qr/parse-pairing-url'
import { SavedInstancesStorage } from '@/lib/storage/saved-instances'

export default function ScanScreen() {
  const router = useRouter()
  const [permission, requestPermission] = useCameraPermissions()
  const scannedRef = useRef(false)
  const [processing, setProcessing] = useState(false)

  if (Platform.OS === 'web') {
    return (
      <ThemedView style={styles.centered}>
        <ThemedText type="subtitle">Not available</ThemedText>
        <ThemedText type="small" themeColor="textSecondary" style={styles.hint}>
          QR scanning is not supported on web.
        </ThemedText>
        <Pressable
          onPress={() => router.back()}
          style={({ pressed }) => [styles.primaryButton, pressed && styles.pressed]}
        >
          <ThemedText type="linkPrimary">Close</ThemedText>
        </Pressable>
      </ThemedView>
    )
  }

  if (!permission) {
    return (
      <ThemedView style={styles.centered}>
        <ThemedText type="small" themeColor="textSecondary">
          Loading camera…
        </ThemedText>
      </ThemedView>
    )
  }

  if (!permission.granted) {
    return (
      <ThemedView style={styles.centered}>
        <ThemedText type="subtitle">Camera access needed</ThemedText>
        <ThemedText type="small" themeColor="textSecondary" style={styles.hint}>
          Allow camera access to scan QR codes from your Canopy desktop.
        </ThemedText>
        <Pressable
          onPress={requestPermission}
          style={({ pressed }) => [styles.primaryButton, pressed && styles.pressed]}
        >
          <ThemedView type="backgroundSelected" style={styles.primaryButtonInner}>
            <ThemedText type="smallBold">Grant permission</ThemedText>
          </ThemedView>
        </Pressable>
        <Pressable
          onPress={() => router.back()}
          style={({ pressed }) => [styles.secondaryButton, pressed && styles.pressed]}
        >
          <ThemedText type="link" themeColor="textSecondary">
            Cancel
          </ThemedText>
        </Pressable>
      </ThemedView>
    )
  }

  const handleScan = async (data: string) => {
    if (scannedRef.current || processing) return
    scannedRef.current = true
    setProcessing(true)

    const parsed = parsePairingUrl(data)
    if (!parsed) {
      Alert.alert('Invalid QR', 'This is not a Canopy pairing code.', [
        {
          text: 'OK',
          onPress: () => {
            scannedRef.current = false
            setProcessing(false)
          },
        },
      ])
      return
    }

    try {
      await SavedInstancesStorage.add({
        nickname: parsed.hostname,
        hostname: parsed.hostname,
        lanIp: parsed.lanIp,
        port: parsed.port,
        token: parsed.token,
      })
      router.back()
    } catch (e) {
      Alert.alert('Could not save', e instanceof Error ? e.message : String(e), [
        {
          text: 'OK',
          onPress: () => {
            scannedRef.current = false
            setProcessing(false)
          },
        },
      ])
    }
  }

  return (
    <View style={styles.container}>
      <CameraView
        style={StyleSheet.absoluteFill}
        facing="back"
        barcodeScannerSettings={{ barcodeTypes: ['qr'] }}
        onBarcodeScanned={({ data }) => handleScan(data)}
      />
      <SafeAreaView style={styles.overlay} edges={['top', 'bottom']}>
        <View style={styles.instructions}>
          <ThemedView type="backgroundElement" style={styles.instructionsPill}>
            <ThemedText type="small">Point your camera at the QR on your Canopy desktop</ThemedText>
          </ThemedView>
        </View>

        <View style={styles.frame}>
          <View style={[styles.corner, styles.topLeft]} />
          <View style={[styles.corner, styles.topRight]} />
          <View style={[styles.corner, styles.bottomLeft]} />
          <View style={[styles.corner, styles.bottomRight]} />
        </View>

        <View style={styles.footer}>
          <Pressable
            onPress={() => router.back()}
            style={({ pressed }) => [styles.cancelButton, pressed && styles.pressed]}
          >
            <ThemedView type="backgroundElement" style={styles.cancelInner}>
              <ThemedText type="smallBold">Cancel</ThemedText>
            </ThemedView>
          </Pressable>
        </View>
      </SafeAreaView>
    </View>
  )
}

const FRAME_SIZE = 260
const CORNER_SIZE = 32
const CORNER_THICK = 3
const CORNER_COLOR = '#ffffff'

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#000',
  },
  overlay: {
    flex: 1,
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  instructions: {
    paddingTop: Spacing.four,
    paddingHorizontal: Spacing.four,
  },
  instructionsPill: {
    paddingVertical: Spacing.two,
    paddingHorizontal: Spacing.three,
    borderRadius: Spacing.five,
  },
  frame: {
    width: FRAME_SIZE,
    height: FRAME_SIZE,
  },
  corner: {
    position: 'absolute',
    width: CORNER_SIZE,
    height: CORNER_SIZE,
    borderColor: CORNER_COLOR,
  },
  topLeft: {
    top: 0,
    left: 0,
    borderTopWidth: CORNER_THICK,
    borderLeftWidth: CORNER_THICK,
    borderTopLeftRadius: Spacing.two,
  },
  topRight: {
    top: 0,
    right: 0,
    borderTopWidth: CORNER_THICK,
    borderRightWidth: CORNER_THICK,
    borderTopRightRadius: Spacing.two,
  },
  bottomLeft: {
    bottom: 0,
    left: 0,
    borderBottomWidth: CORNER_THICK,
    borderLeftWidth: CORNER_THICK,
    borderBottomLeftRadius: Spacing.two,
  },
  bottomRight: {
    bottom: 0,
    right: 0,
    borderBottomWidth: CORNER_THICK,
    borderRightWidth: CORNER_THICK,
    borderBottomRightRadius: Spacing.two,
  },
  footer: {
    paddingBottom: Spacing.four,
  },
  cancelButton: {},
  cancelInner: {
    paddingVertical: Spacing.two,
    paddingHorizontal: Spacing.five,
    borderRadius: Spacing.five,
  },
  centered: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    padding: Spacing.four,
    gap: Spacing.two,
  },
  hint: {
    textAlign: 'center',
    maxWidth: 280,
  },
  primaryButton: {
    marginTop: Spacing.three,
  },
  primaryButtonInner: {
    paddingVertical: Spacing.two,
    paddingHorizontal: Spacing.four,
    borderRadius: Spacing.five,
  },
  secondaryButton: {
    marginTop: Spacing.two,
  },
  pressed: {
    opacity: 0.6,
  },
})
