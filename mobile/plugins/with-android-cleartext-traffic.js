const { withAndroidManifest } = require('expo/config-plugins')

/**
 * Canopy Remote connects to the desktop signaling server over ws:// on a
 * user-selected LAN or Tailscale IP. Android blocks cleartext traffic by
 * default for modern target SDKs, so the WebSocket fails before it can send
 * the QR pairing token unless the application explicitly opts in.
 */
module.exports = function withAndroidCleartextTraffic(config) {
  return withAndroidManifest(config, (cfg) => {
    const application = cfg.modResults.manifest.application?.[0]
    if (!application) {
      throw new Error('AndroidManifest.xml is missing its application element')
    }

    application.$ ??= {}
    application.$['android:usesCleartextTraffic'] = 'true'
    return cfg
  })
}
