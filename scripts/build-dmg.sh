#!/usr/bin/env bash
# Build Yap.app bundle + drag-to-Applications .dmg.
# No external deps — uses cargo, hdiutil, install_name_tool, plutil, qlmanage, sips, iconutil.

set -euo pipefail

APP_NAME="Yap"
BIN_NAME="yap"
BUNDLE_ID="com.yap.app"
VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET_DIR="${ROOT}/target/release"
DIST_DIR="${ROOT}/dist"
APP_DIR="${DIST_DIR}/${APP_NAME}.app"
DMG_STAGE="${DIST_DIR}/dmg-stage"
DMG_PATH="${DIST_DIR}/${APP_NAME}-${VERSION}.dmg"
SVG_SRC="${ROOT}/assets/icons/yap.svg"

echo "==> Building release binary"
cargo build --release --manifest-path "${ROOT}/Cargo.toml"

echo "==> Resetting ${APP_DIR}"
rm -rf "${APP_DIR}" "${DMG_STAGE}" "${DMG_PATH}"
mkdir -p "${APP_DIR}/Contents/MacOS"
mkdir -p "${APP_DIR}/Contents/Resources"
mkdir -p "${APP_DIR}/Contents/Frameworks"

echo "==> Copying binary"
cp "${TARGET_DIR}/${BIN_NAME}" "${APP_DIR}/Contents/MacOS/${BIN_NAME}"
chmod +x "${APP_DIR}/Contents/MacOS/${BIN_NAME}"

echo "==> Bundling dylibs"
for lib in libsherpa-onnx-c-api.dylib libsherpa-onnx-cxx-api.dylib libonnxruntime.1.17.1.dylib libonnxruntime.dylib; do
  if [[ -f "${TARGET_DIR}/${lib}" ]]; then
    cp -a "${TARGET_DIR}/${lib}" "${APP_DIR}/Contents/Frameworks/"
  fi
done

echo "==> Patching rpath"
existing_rpaths=$(otool -l "${APP_DIR}/Contents/MacOS/${BIN_NAME}" | awk '/LC_RPATH/{getline;getline;print $2}' || true)
for rp in ${existing_rpaths}; do
  install_name_tool -delete_rpath "${rp}" "${APP_DIR}/Contents/MacOS/${BIN_NAME}" 2>/dev/null || true
done
install_name_tool -add_rpath "@executable_path/../Frameworks" "${APP_DIR}/Contents/MacOS/${BIN_NAME}"

for dylib in "${APP_DIR}/Contents/Frameworks/"*.dylib; do
  base="$(basename "${dylib}")"
  install_name_tool -id "@rpath/${base}" "${dylib}" 2>/dev/null || true
done

echo "==> Rasterizing SVG → .icns"
if [[ -f "${SVG_SRC}" ]]; then
  ICON_TMP="$(mktemp -d)"
  ICONSET="${ICON_TMP}/${APP_NAME}.iconset"
  mkdir -p "${ICONSET}"

  # qlmanage renders SVG at requested max size into a temp dir.
  qlmanage -t -s 1024 -o "${ICON_TMP}" "${SVG_SRC}" >/dev/null 2>&1
  MASTER="${ICON_TMP}/$(basename "${SVG_SRC}").png"

  if [[ ! -f "${MASTER}" ]]; then
    echo "   qlmanage failed; skipping icon"
  else
    # Apple .iconset slot sizes.
    for spec in "16:icon_16x16.png" "32:icon_16x16@2x.png" "32:icon_32x32.png" "64:icon_32x32@2x.png" \
                "128:icon_128x128.png" "256:icon_128x128@2x.png" "256:icon_256x256.png" \
                "512:icon_256x256@2x.png" "512:icon_512x512.png" "1024:icon_512x512@2x.png"; do
      size="${spec%%:*}"; name="${spec##*:}"
      sips -z "${size}" "${size}" "${MASTER}" --out "${ICONSET}/${name}" >/dev/null
    done
    iconutil -c icns "${ICONSET}" -o "${APP_DIR}/Contents/Resources/${APP_NAME}.icns"
    # Embed SVG source too so the app can read it at runtime if desired.
    cp "${SVG_SRC}" "${APP_DIR}/Contents/Resources/${APP_NAME}.svg"
  fi
  rm -rf "${ICON_TMP}"
fi

echo "==> Writing Info.plist"
ICON_KEY=""
if [[ -f "${APP_DIR}/Contents/Resources/${APP_NAME}.icns" ]]; then
  ICON_KEY="<key>CFBundleIconFile</key><string>${APP_NAME}.icns</string>"
fi

cat > "${APP_DIR}/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>${APP_NAME}</string>
    <key>CFBundleDisplayName</key><string>${APP_NAME}</string>
    <key>CFBundleExecutable</key><string>${BIN_NAME}</string>
    <key>CFBundleIdentifier</key><string>${BUNDLE_ID}</string>
    <key>CFBundleVersion</key><string>${VERSION}</string>
    <key>CFBundleShortVersionString</key><string>${VERSION}</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>LSMinimumSystemVersion</key><string>12.0</string>
    <key>LSUIElement</key><true/>
    <key>NSHighResolutionCapable</key><true/>
    ${ICON_KEY}
    <key>NSMicrophoneUsageDescription</key><string>Yap listens to your microphone to transcribe push-to-talk dictation.</string>
    <key>NSAppleEventsUsageDescription</key><string>Yap pastes transcribed text into the active app.</string>
</dict>
</plist>
PLIST
plutil -lint "${APP_DIR}/Contents/Info.plist" >/dev/null

echo "==> Ad-hoc codesign"
codesign --force --deep --sign - "${APP_DIR}" >/dev/null 2>&1 || \
  echo "   (codesign failed — app will still launch but Gatekeeper may complain)"

echo "==> Staging DMG"
mkdir -p "${DMG_STAGE}"
cp -R "${APP_DIR}" "${DMG_STAGE}/"
ln -s /Applications "${DMG_STAGE}/Applications"

echo "==> Creating ${DMG_PATH}"
hdiutil create \
  -volname "${APP_NAME} ${VERSION}" \
  -srcfolder "${DMG_STAGE}" \
  -ov -format UDZO \
  "${DMG_PATH}" >/dev/null

rm -rf "${DMG_STAGE}"

echo "==> Done: ${DMG_PATH}"
du -h "${DMG_PATH}" | awk '{print "   size: "$1}'
