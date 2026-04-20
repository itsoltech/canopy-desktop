const { withInfoPlist } = require('expo/config-plugins')

/**
 * Strip iOS Info.plist keys injected by @config-plugins/react-native-webrtc
 * that we don't need. Mobile uses WebRTC data channels only — never
 * getUserMedia — so NSCameraUsageDescription and NSMicrophoneUsageDescription
 * would just invite App Store review questions about audio/video features
 * we don't ship.
 *
 * Must run after the webrtc plugin in `plugins` order so it can delete
 * the keys the plugin wrote.
 */
module.exports = function withStripWebRTCiOSPermissions(config) {
  return withInfoPlist(config, (cfg) => {
    delete cfg.modResults.NSCameraUsageDescription
    delete cfg.modResults.NSMicrophoneUsageDescription
    return cfg
  })
}
