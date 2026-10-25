import { requireOptionalNativeModule } from 'expo-modules-core'

import type { NativeRemote } from './bridge'
export type { NativeRemote, RpcEvent } from './bridge'

let native: NativeRemote | null | undefined
export function nativeRemote(): NativeRemote {
  native ??= requireOptionalNativeModule<NativeRemote>('CanopyRemote')
  if (!native) throw Error('Native Canopy transport requires a development or production build with CanopyRemote; Expo Go and web are not supported.')
  return native
}
