const { withInfoPlist } = require('expo/config-plugins')

/**
 * Strip iOS Info.plist keys injected by @config-plugins/react-native-webrtc
 * that we don't need. Mobile uses WebRTC data channels only — never audio
 * getUserMedia — so NSMicrophoneUsageDescription would invite App Store
 * review questions about an audio feature we don't ship. Camera access is
 * intentionally preserved because pairing scans QR codes.
 *
 * Must run after the webrtc plugin in `plugins` order so it can delete
 * the keys the plugin wrote.
 */
module.exports = function withStripWebRTCiOSPermissions(config) {
  return withInfoPlist(config, (cfg) => {
    delete cfg.modResults.NSMicrophoneUsageDescription
    return cfg
  })
}
