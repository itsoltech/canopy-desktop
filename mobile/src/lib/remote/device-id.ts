import * as SecureStore from 'expo-secure-store'

/**
 * Persistent device identifier sent in the `pair` message so the host can
 * recognize the same physical device across reconnects. The host uses this
 * to skip the accept modal on return visits when the user clicked "Remember
 * this device". The ID lives in SecureStore and survives app updates but
 * not full reinstalls.
 */
const DEVICE_ID_STORAGE_KEY = 'canopy.remote.deviceId'

function randomHex(bytes: number): string {
  const arr = new Uint8Array(bytes)
  crypto.getRandomValues(arr)
  let hex = ''
  for (let i = 0; i < arr.length; i++) {
    hex += arr[i].toString(16).padStart(2, '0')
  }
  return hex
}

export async function loadOrCreateDeviceId(): Promise<string> {
  try {
    const existing = await SecureStore.getItemAsync(DEVICE_ID_STORAGE_KEY)
    if (existing && /^[0-9a-f]+$/i.test(existing) && existing.length >= 16) {
      return existing
    }
    const fresh = randomHex(16)
    await SecureStore.setItemAsync(DEVICE_ID_STORAGE_KEY, fresh)
    return fresh
  } catch {
    return randomHex(16)
  }
}
