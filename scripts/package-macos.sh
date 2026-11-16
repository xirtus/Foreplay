#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

VERSION="${FOREPLAY_VERSION:-1.1.0}"
DIST_DIR="$ROOT_DIR/dist/macos"
ICON_DIR="$ROOT_DIR/dist/icons"
APP_DIR="$DIST_DIR/Foreplay.app"

echo "=== [1/5] Generating Retina Foreplay.icns ==="
python3 "$ROOT_DIR/scripts/generate_icons.py" "$ICON_DIR"

echo "=== [2/5] Building Release Binaries for macOS arm64 (aarch64-apple-darwin) ==="
cargo build --release --target aarch64-apple-darwin \
  -p foreplay-editor \
  -p foreplay-player \
  -p foreplay-harness \
  -p foreplay-net

BUILD_UNIVERSAL=0
if [[ "${FOREPLAY_UNIVERSAL:-1}" == "1" ]] && rustup target list --installed | grep -q "x86_64-apple-darwin"; then
  echo "=== [2b/5] Building Release Binaries for macOS Intel (x86_64-apple-darwin) for Universal2 ==="
  cargo build --release --target x86_64-apple-darwin \
    -p foreplay-editor \
    -p foreplay-player \
    -p foreplay-harness \
    -p foreplay-net
  BUILD_UNIVERSAL=1
fi

echo "=== [3/5] Assembling Foreplay.app Bundle ==="
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"

install_macho() {
  local src_name="$1"
  local dst_name="$2"
  local arm_bin="$ROOT_DIR/target/aarch64-apple-darwin/release/$src_name"
  local dst_bin="$APP_DIR/Contents/MacOS/$dst_name"
  if [[ "$BUILD_UNIVERSAL" == "1" ]]; then
    local x64_bin="$ROOT_DIR/target/x86_64-apple-darwin/release/$src_name"
    lipo -create -output "$dst_bin" "$arm_bin" "$x64_bin"
  else
    cp "$arm_bin" "$dst_bin"
  fi
  strip -x "$dst_bin" 2>/dev/null || true
  chmod +x "$dst_bin"
}

install_macho "foreplay-editor" "Foreplay"
install_macho "foreplay-player" "foreplay-player"
install_macho "foreplay-harness" "foreplay-harness"
install_macho "foreplay-server" "foreplay-server"

cp "$ICON_DIR/Foreplay.icns" "$APP_DIR/Contents/Resources/Foreplay.icns"
if [[ -d "$ROOT_DIR/templates" ]]; then
  cp -R "$ROOT_DIR/templates" "$APP_DIR/Contents/Resources/templates"
fi

cat > "$APP_DIR/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleDisplayName</key>
    <string>Foreplay</string>
    <key>CFBundleExecutable</key>
    <string>Foreplay</string>
    <key>CFBundleIconFile</key>
    <string>Foreplay</string>
    <key>CFBundleIdentifier</key>
    <string>dev.sexos.sexlife.foreplay</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>Foreplay</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.developer-tools</string>
    <key>LSMinimumSystemVersion</key>
    <string>13.0</string>
    <key>LSSupportsGameMode</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeExtensions</key>
            <array>
                <string>foreplay</string>
                <string>fscn</string>
                <string>tscn</string>
            </array>
            <key>CFBundleTypeName</key>
            <string>Foreplay / Godot Scene Document</string>
            <key>CFBundleTypeRole</key>
            <string>Editor</string>
        </dict>
    </array>
</dict>
</plist>
EOF

echo "=== [4/5] Ad-Hoc Code Signing & Verifying Foreplay.app ==="
codesign --force --deep --sign - "$APP_DIR"
codesign --verify --deep --strict "$APP_DIR"

ARCH_TAG="arm64"
if [[ "$BUILD_UNIVERSAL" == "1" ]]; then
  ARCH_TAG="universal"
fi

DMG_STAGE="$DIST_DIR/dmg-staging"
DMG_PATH="$DIST_DIR/Foreplay-${VERSION}-macos-${ARCH_TAG}.dmg"
ZIP_PATH="$DIST_DIR/Foreplay-${VERSION}-macos-${ARCH_TAG}.zip"

echo "=== [5/5] Creating macOS .dmg Installer & .zip Archive ==="
rm -rf "$DMG_STAGE" "$DMG_PATH" "$ZIP_PATH"
mkdir -p "$DMG_STAGE"
cp -R "$APP_DIR" "$DMG_STAGE/Foreplay.app"
ln -s /Applications "$DMG_STAGE/Applications"

hdiutil create \
  -volname "Foreplay ${VERSION}" \
  -srcfolder "$DMG_STAGE" \
  -ov \
  -format UDZO \
  "$DMG_PATH" >/dev/null
rm -rf "$DMG_STAGE"

ditto -c -k --sequesterRsrc --keepParent "$APP_DIR" "$ZIP_PATH"

echo "macOS packaging complete:"
echo "  App Bundle : $APP_DIR"
echo "  Disk Image : $DMG_PATH"
echo "  Zip Archive: $ZIP_PATH"
