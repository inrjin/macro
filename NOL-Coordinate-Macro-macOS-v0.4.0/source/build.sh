#!/bin/bash
set -euo pipefail
MAC_ROOT="$(cd "$(dirname "$0")" && pwd)"
APP="$MAC_ROOT/build/NOL Coordinate Macro.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$MAC_ROOT/build/arm64" "$MAC_ROOT/build/x86_64"
for arch in arm64 x86_64; do
  swiftc -O -parse-as-library -target "$arch-apple-macosx13.0" \
    "$MAC_ROOT/Sources/Core.swift" "$MAC_ROOT/Sources/Runtime.swift" \
    "$MAC_ROOT/Sources/Tests.swift" "$MAC_ROOT/Sources/App.swift" \
    -o "$MAC_ROOT/build/$arch/NolCoordinateMacro" \
    -framework SwiftUI -framework AppKit -framework ScreenCaptureKit -framework Carbon
done
lipo -create "$MAC_ROOT/build/arm64/NolCoordinateMacro" "$MAC_ROOT/build/x86_64/NolCoordinateMacro" \
  -output "$APP/Contents/MacOS/NolCoordinateMacro"
cp "$MAC_ROOT/Info.plist" "$APP/Contents/Info.plist"
cp "$MAC_ROOT/Resources/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"
codesign --force --sign - "$APP"
codesign --verify --deep --strict "$APP"
"$APP/Contents/MacOS/NolCoordinateMacro" --self-test
