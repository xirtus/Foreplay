#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

VERSION="${FOREPLAY_VERSION:-1.1.0}"
DIST_DIR="$ROOT_DIR/dist/linux"
ICON_DIR="$ROOT_DIR/dist/icons"
mkdir -p "$DIST_DIR"

echo "=== [1/4] Generating Linux Hicolor Icons ==="
python3 "$ROOT_DIR/scripts/generate_icons.py" "$ICON_DIR"

HOST_OS="$(uname -s)"

setup_cross_sysroot() {
  local zig_bin="$1"
  local zig_target="$2"
  local sysroot_dir="$ROOT_DIR/target/linux-cross-stubs/$zig_target"
  mkdir -p "$sysroot_dir/lib" "$sysroot_dir/pkgconfig" "$sysroot_dir/bin" "$sysroot_dir/src"

  # Ensure alsa-sys and libudev-sys crate archives are fetched in ~/.cargo/registry/cache
  cargo fetch >/dev/null

  python3 - "$sysroot_dir/src" <<'PY'
import glob, os, re, sys, tarfile

out_dir = sys.argv[1]
cache_root = os.path.expanduser("~/.cargo/registry/cache")

def extract_symbols(crate_prefix: str):
    fns, statics = set(), set()
    matches = glob.glob(os.path.join(cache_root, "*", f"{crate_prefix}-*.crate"))
    for crate_tar in matches:
        with tarfile.open(crate_tar, "r:gz") as tf:
            for member in tf.getmembers():
                if member.name.endswith(".rs"):
                    f = tf.extractfile(member)
                    if not f:
                        continue
                    txt = f.read().decode("utf-8", errors="ignore")
                    for m in re.finditer(r"\bpub\s+fn\s+([A-Za-z0-9_]+)\s*\(", txt):
                        fns.add(m.group(1))
                    for m in re.finditer(r"\bpub\s+static\s+(?:mut\s+)?([A-Za-z0-9_]+)\s*:", txt):
                        statics.add(m.group(1))
    fns -= statics
    return sorted(fns), sorted(statics)

for prefix, c_name in [("alsa-sys", "alsa_stub.c"), ("libudev-sys", "udev_stub.c")]:
    fns, statics = extract_symbols(prefix)
    with open(os.path.join(out_dir, c_name), "w") as out:
        for s in statics:
            out.write(f"__attribute__((visibility(\"default\"))) long {s} = 0;\n")
        for fn in fns:
            out.write(f"__attribute__((visibility(\"default\"))) void {fn}(void) {{}}\n")
PY

  "$zig_bin" cc -shared -fPIC -target "$zig_target" \
    -Wl,-soname,libasound.so.2 \
    -o "$sysroot_dir/lib/libasound.so.2" \
    "$sysroot_dir/src/alsa_stub.c"
  ln -sf libasound.so.2 "$sysroot_dir/lib/libasound.so"

  "$zig_bin" cc -shared -fPIC -target "$zig_target" \
    -Wl,-soname,libudev.so.1 \
    -o "$sysroot_dir/lib/libudev.so.1" \
    "$sysroot_dir/src/udev_stub.c"
  ln -sf libudev.so.1 "$sysroot_dir/lib/libudev.so"

  cat > "$sysroot_dir/pkgconfig/alsa.pc" <<EOF
prefix=$sysroot_dir
libdir=\${prefix}/lib
Name: alsa
Description: ALSA link stub for cross-compilation
Version: 1.2.10
Libs: -L\${libdir} -lasound
EOF

  cat > "$sysroot_dir/pkgconfig/libudev.pc" <<EOF
prefix=$sysroot_dir
libdir=\${prefix}/lib
Name: libudev
Description: libudev link stub for cross-compilation
Version: 252
Libs: -L\${libdir} -ludev
EOF

  # Wrapper filtering flags unsupported by zig cc linker driver
  cat > "$sysroot_dir/bin/zig-cc-linker" <<EOF
#!/usr/bin/env bash
ARGS=()
for arg in "\$@"; do
  case "\$arg" in
    -Wl,--eh-frame-hdr) ;;
    *) ARGS+=("\$arg") ;;
  esac
done
exec "$zig_bin" cc -target "$zig_target" -L"$sysroot_dir/lib" -Wl,--allow-shlib-undefined "\${ARGS[@]}"
EOF
  chmod +x "$sysroot_dir/bin/zig-cc-linker"

  cat > "$sysroot_dir/bin/zig-ar" <<EOF
#!/usr/bin/env bash
exec "$zig_bin" ar "\$@"
EOF
  chmod +x "$sysroot_dir/bin/zig-ar"

  echo "$sysroot_dir"
}

build_and_bundle_linux_arch() {
  local rust_target="$1"
  local zig_target="$2"
  local arch_label="$3"
  local deb_arch="$4"

  echo "=== [2/4] Building Release Binaries for Linux ${arch_label} (${rust_target}) ==="
  if [[ "$HOST_OS" == "Linux" && -z "${ZIG_BIN:-}" ]]; then
    cargo build --release --target "$rust_target" \
      -p foreplay-editor \
      -p foreplay-player \
      -p foreplay-harness \
      -p foreplay-net
  else
    local zig_bin="${ZIG_BIN:-$(command -v zig || true)}"
    if [[ -z "$zig_bin" ]]; then
      echo "Error: ZIG_BIN or zig in PATH is required to cross-compile Linux binaries from $HOST_OS" >&2
      exit 1
    fi
    local sysroot_dir
    sysroot_dir="$(setup_cross_sysroot "$zig_bin" "$zig_target")"
    local upper_target
    upper_target="$(echo "$rust_target" | tr '[:lower:]-' '[:upper:]_')"

    env \
      PKG_CONFIG_ALLOW_CROSS=1 \
      PKG_CONFIG_LIBDIR="$sysroot_dir/pkgconfig" \
      PKG_CONFIG_PATH="" \
      "CC_${rust_target//-/_}=$sysroot_dir/bin/zig-cc-linker" \
      "AR_${rust_target//-/_}=$sysroot_dir/bin/zig-ar" \
      "CARGO_TARGET_${upper_target}_LINKER=$sysroot_dir/bin/zig-cc-linker" \
      cargo build --release --target "$rust_target" \
        -p foreplay-editor \
        -p foreplay-player \
        -p foreplay-harness \
        -p foreplay-net
  fi

  echo "=== [3/4] Assembling Linux AppDir, Tarball & .deb for ${arch_label} ==="
  local bundle_name="Foreplay-${VERSION}-linux-${arch_label}"
  local appdir="$DIST_DIR/$bundle_name"
  rm -rf "$appdir"
  mkdir -p \
    "$appdir/bin" \
    "$appdir/share/applications" \
    "$appdir/share/metainfo" \
    "$appdir/share/icons/hicolor/64x64/apps" \
    "$appdir/share/icons/hicolor/128x128/apps" \
    "$appdir/share/icons/hicolor/256x256/apps" \
    "$appdir/share/icons/hicolor/512x512/apps"

  cp "$ROOT_DIR/target/$rust_target/release/foreplay-editor" "$appdir/Foreplay"
  cp "$ROOT_DIR/target/$rust_target/release/foreplay-editor" "$appdir/bin/foreplay-editor"
  cp "$ROOT_DIR/target/$rust_target/release/foreplay-player" "$appdir/bin/foreplay-player"
  cp "$ROOT_DIR/target/$rust_target/release/foreplay-harness" "$appdir/bin/foreplay-harness"
  cp "$ROOT_DIR/target/$rust_target/release/foreplay-server" "$appdir/bin/foreplay-server"
  chmod +x "$appdir/Foreplay" "$appdir/bin/"*

  cp "$ICON_DIR/foreplay_64.png" "$appdir/share/icons/hicolor/64x64/apps/dev.sexos.foreplay.png"
  cp "$ICON_DIR/foreplay_128.png" "$appdir/share/icons/hicolor/128x128/apps/dev.sexos.foreplay.png"
  cp "$ICON_DIR/foreplay_256.png" "$appdir/share/icons/hicolor/256x256/apps/dev.sexos.foreplay.png"
  cp "$ICON_DIR/foreplay_512.png" "$appdir/share/icons/hicolor/512x512/apps/dev.sexos.foreplay.png"
  cp "$ICON_DIR/foreplay_256.png" "$appdir/dev.sexos.foreplay.png"

  if [[ -d "$ROOT_DIR/templates" ]]; then
    cp -R "$ROOT_DIR/templates" "$appdir/templates"
  fi

  cat > "$appdir/AppRun" <<'EOF'
#!/usr/bin/env bash
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PATH="$HERE/bin:$PATH"
exec "$HERE/Foreplay" --gui "$@"
EOF
  chmod +x "$appdir/AppRun"

  cat > "$appdir/share/applications/dev.sexos.foreplay.desktop" <<EOF
[Desktop Entry]
Type=Application
Version=1.0
Name=Foreplay Game Engine
GenericName=Game Engine & 3D Studio
Comment=Godot 4-compatible Rust & Three.js Game Engine, Polyamour 3D Modeler & Studio IDE
Exec=Foreplay %f
Icon=dev.sexos.foreplay
Terminal=false
Categories=Development;IDE;3DGraphics;Game;
MimeType=application/x-foreplay-scene;application/x-godot-scene;
StartupWMClass=dev.sexos.foreplay
Keywords=Godot;Rust;Bevy;ThreeJS;GameEngine;3D;Polyamour;
EOF
  cp "$appdir/share/applications/dev.sexos.foreplay.desktop" "$appdir/dev.sexos.foreplay.desktop"

  cat > "$appdir/share/metainfo/dev.sexos.foreplay.appdata.xml" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>dev.sexos.foreplay</id>
  <name>Foreplay Game Engine</name>
  <summary>AI-native Rust &amp; Three.js game engine, Godot 4 studio, and Polyamour 3D modeler</summary>
  <metadata_license>CC0-1.0</metadata_license>
  <project_license>GPL-3.0-only AND MPL-2.0</project_license>
  <launchable type="desktop-id">dev.sexos.foreplay.desktop</launchable>
  <url type="homepage">https://github.com/xirtus/Foreplay</url>
  <releases>
    <release version="${VERSION}" date="2026-11-16"/>
  </releases>
</component>
EOF

  cat > "$appdir/install.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFIX="${PREFIX:-$HOME/.local}"
mkdir -p "$PREFIX/bin" "$PREFIX/share/applications" "$PREFIX/share/metainfo"
cp "$HERE/Foreplay" "$PREFIX/bin/foreplay"
cp "$HERE/bin/"* "$PREFIX/bin/"
for sz in 64x64 128x128 256x256 512x512; do
  mkdir -p "$PREFIX/share/icons/hicolor/$sz/apps"
  cp "$HERE/share/icons/hicolor/$sz/apps/dev.sexos.foreplay.png" "$PREFIX/share/icons/hicolor/$sz/apps/"
done
sed "s|Exec=Foreplay|Exec=$PREFIX/bin/foreplay|g" \
  "$HERE/share/applications/dev.sexos.foreplay.desktop" \
  > "$PREFIX/share/applications/dev.sexos.foreplay.desktop"
cp "$HERE/share/metainfo/dev.sexos.foreplay.appdata.xml" "$PREFIX/share/metainfo/"
echo "Installed Foreplay Game Engine into $PREFIX"
EOF
  chmod +x "$appdir/install.sh"

  local tar_path="$DIST_DIR/${bundle_name}.tar.gz"
  tar -C "$DIST_DIR" -czf "$tar_path" "$bundle_name"

  local deb_path="$DIST_DIR/foreplay_${VERSION}_${deb_arch}.deb"
  python3 "$ROOT_DIR/scripts/build_deb.py" "$appdir" "$VERSION" "$deb_arch" "$deb_path"

  echo "Linux (${arch_label}) packaging complete:"
  echo "  Portable Dir : $appdir"
  echo "  Tarball      : $tar_path"
  echo "  Debian .deb  : $deb_path"
}

TARGETS="${FOREPLAY_LINUX_ARCHS:-x86_64 arm64}"
for arch in $TARGETS; do
  case "$arch" in
    x86_64|amd64)
      build_and_bundle_linux_arch "x86_64-unknown-linux-gnu" "x86_64-linux-gnu.2.28" "x86_64" "amd64"
      ;;
    arm64|aarch64)
      build_and_bundle_linux_arch "aarch64-unknown-linux-gnu" "aarch64-linux-gnu.2.28" "arm64" "arm64"
      ;;
    *)
      echo "Unknown architecture: $arch" >&2
      exit 1
      ;;
  esac
done
