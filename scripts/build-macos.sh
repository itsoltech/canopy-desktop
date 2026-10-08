#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
profile="${1:-release}"
if [ "$#" -gt 0 ]; then shift; fi
case "$profile" in
  release) app_name='Canopy'; app_id='tech.itsol.canopy.rust.release' ;;
  profiling) app_name='Canopy Profile'; app_id='tech.itsol.canopy.rust.profile' ;;
  dev) app_name='Canopy Dev'; app_id='tech.itsol.canopy.rust.dev' ;;
  *) echo 'Usage: build-macos.sh [release|profiling|dev] [Cargo options]' >&2; exit 2 ;;
esac
cargo build --locked --profile "$profile" "$@"
output="$profile"
if [ "$profile" = dev ]; then output=debug; fi
bundle="$PWD/target/$output/$app_name.app"
mkdir -p "$bundle/Contents/MacOS"
# Replace the inode atomically so a running app and macOS code-signature cache
# never see a partially overwritten executable.
cp "target/$output/canopy-desktop" "$bundle/Contents/MacOS/canopy-desktop.next"
mv -f "$bundle/Contents/MacOS/canopy-desktop.next" "$bundle/Contents/MacOS/canopy-desktop"
cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>canopy-desktop</string>
<key>CFBundleIdentifier</key><string>$app_id</string>
<key>CFBundleName</key><string>$app_name</string>
<key>CFBundleDisplayName</key><string>$app_name</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>1</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
codesign --force --sign - "$bundle"
printf '%s\n' "$bundle"
