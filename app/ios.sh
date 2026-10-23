#!/usr/bin/env bash
# Packs and launches the app in the iOS simulator: a debug
# aarch64-apple-ios-sim build, a minimal .app around it, install + launch.
#
#   ./ios.sh [device name]     default: iPhone 17 Pro
#
# Live edits in the simulator: the build goes through `cargo ra-jit` so the
# daemon knows the binary; the app links ra-jit-live, whose macOS path also
# covers the simulator (a sim process is a macOS arm64 process — only a real
# device lacks writable+executable pages). RA_JIT_HOME points the app at the
# host daemon's live directory. An edit is `cargo ra-jit build --target
# aarch64-apple-ios-sim`, answered with a patch in the running sim app.
set -e
cd "$(dirname "$0")"

TARGET=aarch64-apple-ios-sim
APP=StackNative
ID=ai.stacknative.app
BUNDLE="target/ios-sim/$APP.app"

if ! cargo ra-jit build --target "$TARGET" 2>&1 | tee /tmp/rajit-ios.log | grep -q "Finished"; then
    cat /tmp/rajit-ios.log
fi
BIN="target/$TARGET/debug/stack-native-app"

rm -rf "$BUNDLE"
mkdir -p "$BUNDLE"
cp "$BIN" "$BUNDLE/$APP"
cat > "$BUNDLE/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>$APP</string>
    <key>CFBundleIdentifier</key><string>$ID</string>
    <key>CFBundleName</key><string>$APP</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>CFBundleShortVersionString</key><string>1.0</string>
    <key>MinimumOSVersion</key><string>15.0</string>
    <key>UILaunchStoryboardName</key><string></string>
    <key>UIDeviceFamily</key><array><integer>1</integer><integer>2</integer></array>
    <key>UISupportedInterfaceOrientations</key>
    <array>
        <string>UIInterfaceOrientationPortrait</string>
        <string>UIInterfaceOrientationLandscapeLeft</string>
        <string>UIInterfaceOrientationLandscapeRight</string>
    </array>
</dict>
</plist>
EOF
codesign --force --sign - "$BUNDLE/$APP" 2>/dev/null || true

NAME="${1:-iPhone 17 Pro}"
DEVICE="$(xcrun simctl list devices available | grep "$NAME" | head -1 | sed -E 's/.*\(([0-9A-F-]+)\).*/\1/')"
if [ -z "$DEVICE" ]; then
    echo "no available device named '$NAME'; pick one:" >&2
    xcrun simctl list devices available | grep iPhone >&2
    exit 1
fi

xcrun simctl boot "$DEVICE" 2>/dev/null || true
open -a Simulator --args -CurrentDeviceUDID "$DEVICE" 2>/dev/null || true
xcrun simctl install "$DEVICE" "$BUNDLE"
SIMCTL_CHILD_NSH_DEVELOPMENT=1 \
SIMCTL_CHILD_RA_JIT_HOME="$HOME/.cache/ra-jit" \
    xcrun simctl launch --console-pty "$DEVICE" "$ID"
