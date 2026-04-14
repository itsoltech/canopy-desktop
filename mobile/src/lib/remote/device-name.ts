import * as Device from 'expo-device'
import { Platform } from 'react-native'

/**
 * Friendly device label sent in the `pair` message. Shown on the desktop
 * accept-device modal so the user can tell devices apart ("Damian's iPhone"
 * vs "iPad Pro"). Falls back to the OS default if expo-device doesn't
 * provide a user-assigned name.
 */
export function defaultDeviceName(): string {
  const name = Device.deviceName?.trim()
  if (name) return name
  const model = Device.modelName?.trim()
  if (model) return model
  if (Platform.OS === 'ios') return 'iPhone'
  if (Platform.OS === 'android') return 'Android device'
  return 'Mobile device'
}
