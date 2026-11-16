#!/usr/bin/env python3
"""Build a spec-compliant Debian/Ubuntu .deb package without requiring dpkg-deb on macOS."""

import gzip
import hashlib
import io
import os
import stat
import sys
import tarfile
import time


def make_ar_member(name: str, payload: bytes, mtime: int) -> bytes:
    header = (
        f"{name:<16}"
        f"{mtime:<12}"
        f"{0:<6}"
        f"{0:<6}"
        f"{0o100644:<8o}"
        f"{len(payload):<10}"
        "`\n"
    ).encode("ascii")
    if len(payload) % 2 == 1:
        return header + payload + b"\n"
    return header + payload


def add_tar_bytes(tf: tarfile.TarFile, arcname: str, data: bytes, mode: int, mtime: int) -> None:
    ti = tarfile.TarInfo(name=arcname)
    ti.size = len(data)
    ti.mtime = mtime
    ti.mode = mode
    ti.uid = 0
    ti.gid = 0
    ti.uname = "root"
    ti.gname = "root"
    tf.addfile(ti, io.BytesIO(data))


def add_tar_dir(tf: tarfile.TarFile, arcname: str, mtime: int) -> None:
    ti = tarfile.TarInfo(name=arcname)
    ti.type = tarfile.DIRTYPE
    ti.mode = 0o755
    ti.mtime = mtime
    ti.uid = 0
    ti.gid = 0
    ti.uname = "root"
    ti.gname = "root"
    tf.addfile(ti)


def build_deb(appdir: str, version: str, deb_arch: str, out_deb: str) -> None:
    mtime = int(time.time())
    files_to_pack: list[tuple[str, bytes, int]] = []

    bin_map = {
        os.path.join(appdir, "Foreplay"): ("./usr/bin/foreplay", 0o755),
        os.path.join(appdir, "bin/foreplay-editor"): ("./usr/bin/foreplay-editor", 0o755),
        os.path.join(appdir, "bin/foreplay-player"): ("./usr/bin/foreplay-player", 0o755),
        os.path.join(appdir, "bin/foreplay-harness"): ("./usr/bin/foreplay-harness", 0o755),
        os.path.join(appdir, "bin/foreplay-server"): ("./usr/bin/foreplay-server", 0o755),
        os.path.join(appdir, "share/applications/dev.sexos.foreplay.desktop"): (
            "./usr/share/applications/dev.sexos.foreplay.desktop",
            0o644,
        ),
        os.path.join(appdir, "share/metainfo/dev.sexos.foreplay.appdata.xml"): (
            "./usr/share/metainfo/dev.sexos.foreplay.appdata.xml",
            0o644,
        ),
    }

    for sz in ["64x64", "128x128", "256x256", "512x512"]:
        icon_src = os.path.join(appdir, f"share/icons/hicolor/{sz}/apps/dev.sexos.foreplay.png")
        if os.path.isfile(icon_src):
            bin_map[icon_src] = (f"./usr/share/icons/hicolor/{sz}/apps/dev.sexos.foreplay.png", 0o644)

    installed_bytes = 0
    md5_lines = []
    for src, (dst, mode) in sorted(bin_map.items(), key=lambda x: x[1][0]):
        with open(src, "rb") as f:
            raw = f.read()
        if dst == "./usr/share/applications/dev.sexos.foreplay.desktop":
            raw = raw.replace(b"Exec=Foreplay", b"Exec=/usr/bin/foreplay")
        installed_bytes += len(raw)
        md5_hex = hashlib.md5(raw).hexdigest()
        md5_lines.append(f"{md5_hex}  {dst[2:]}")
        files_to_pack.append((dst, raw, mode))

    # Build data.tar.gz
    data_buf = io.BytesIO()
    created_dirs = set()
    with gzip.GzipFile(filename="", mode="wb", fileobj=data_buf, mtime=mtime) as gz:
        with tarfile.open(mode="w", fileobj=gz, format=tarfile.GNU_FORMAT) as tf:
            add_tar_dir(tf, ".", mtime)
            created_dirs.add(".")
            for dst, raw, mode in files_to_pack:
                parts = dst.split("/")
                for i in range(2, len(parts)):
                    d = "/".join(parts[:i])
                    if d not in created_dirs:
                        add_tar_dir(tf, d, mtime)
                        created_dirs.add(d)
                add_tar_bytes(tf, dst, raw, mode, mtime)
    data_tar_gz = data_buf.getvalue()

    installed_kb = max(1, (installed_bytes + 1023) // 1024)
    control_text = (
        f"Package: foreplay\n"
        f"Version: {version}\n"
        f"Section: devel\n"
        f"Priority: optional\n"
        f"Architecture: {deb_arch}\n"
        f"Installed-Size: {installed_kb}\n"
        f"Depends: libc6 (>= 2.28), libasound2t64 | libasound2, libudev1, libvulkan1 | mesa-vulkan-drivers\n"
        f"Maintainer: SexOS & sexlife Studio <https://github.com/xirtus/Foreplay>\n"
        f"Homepage: https://github.com/xirtus/Foreplay\n"
        f"Description: Foreplay Game Engine — Godot 4-compatible Rust & Three.js Studio IDE\n"
        f" AI-native Rust & 3JSE Three.js game engine, built-in Polyamour 3D modeling suite,\n"
        f" GDScript 2.0 + LSP + GDExtension runtime, and standalone game player.\n"
    ).encode("utf-8")
    md5sums_text = ("\n".join(md5_lines) + "\n").encode("utf-8")

    control_buf = io.BytesIO()
    with gzip.GzipFile(filename="", mode="wb", fileobj=control_buf, mtime=mtime) as gz:
        with tarfile.open(mode="w", fileobj=gz, format=tarfile.GNU_FORMAT) as tf:
            add_tar_dir(tf, ".", mtime)
            add_tar_bytes(tf, "./control", control_text, 0o644, mtime)
            add_tar_bytes(tf, "./md5sums", md5sums_text, 0o644, mtime)
    control_tar_gz = control_buf.getvalue()

    ar_bytes = (
        b"!<arch>\n"
        + make_ar_member("debian-binary", b"2.0\n", mtime)
        + make_ar_member("control.tar.gz", control_tar_gz, mtime)
        + make_ar_member("data.tar.gz", data_tar_gz, mtime)
    )
    os.makedirs(os.path.dirname(out_deb), exist_ok=True)
    with open(out_deb, "wb") as f:
        f.write(ar_bytes)
    os.chmod(out_deb, stat.S_IRUSR | stat.S_IWUSR | stat.S_IRGRP | stat.S_IROTH)


if __name__ == "__main__":
    build_deb(sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4])
