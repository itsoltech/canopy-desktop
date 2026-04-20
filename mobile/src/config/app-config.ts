export type UpdateChannel = 'development' | 'production'

export class AppConfig {
  public static readonly CHANNEL: UpdateChannel =
    (process.env.EXPO_PUBLIC_CHANNEL as UpdateChannel) || 'development'
}
