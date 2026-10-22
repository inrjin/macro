#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
APP="$PWD/build/NOL Ticket Assistant.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" build/arm64 build/x86_64
for ARCH in arm64 x86_64; do
  swiftc -O -parse-as-library -target "$ARCH-apple-macosx13.0" Sources/Shared.swift Sources/NativeHost.swift -o "build/$ARCH/NolTicketNativeHost"
  swiftc -O -parse-as-library -target "$ARCH-apple-macosx13.0" Sources/Shared.swift Sources/App.swift -o "build/$ARCH/NolTicketAssistant" -framework SwiftUI -framework AppKit
 done
lipo -create build/arm64/NolTicketNativeHost build/x86_64/NolTicketNativeHost -output "$APP/Contents/MacOS/NolTicketNativeHost"
lipo -create build/arm64/NolTicketAssistant build/x86_64/NolTicketAssistant -output "$APP/Contents/MacOS/NolTicketAssistant"
cp -R ../extension "$APP/Contents/Resources/"
cp ../rehearsal-competition.html "$APP/Contents/Resources/"
swift Sources/Icon.swift "$PWD/build/AppIcon.iconset"
python3 package-icon.py build/AppIcon.iconset "$APP/Contents/Resources/AppIcon.icns"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.nol-ticket-assistant.mac</string>
<key>CFBundleName</key><string>NOL Ticket Assistant</string>
<key>CFBundleDisplayName</key><string>NOL Ticket Assistant</string>
<key>CFBundleExecutable</key><string>NolTicketAssistant</string>
<key>CFBundleIconFile</key><string>AppIcon</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.2.0</string>
<key>CFBundleVersion</key><string>2</string>
<key>LSMinimumSystemVersion</key><string>13.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSPrincipalClass</key><string>NSApplication</string>
</dict></plist>
PLIST
codesign --force --sign - "$APP/Contents/MacOS/NolTicketNativeHost"
codesign --force --sign - "$APP"
"$APP/Contents/MacOS/NolTicketNativeHost" --self-test
"$APP/Contents/MacOS/NolTicketAssistant" --self-test
codesign --verify --deep --strict "$APP"
printf 'Built: %s\n' "$APP"
