#!/usr/bin/env python3
"""Generate high-DPI Studio Obsidian & Chroma icons for Foreplay (macOS .icns + Linux PNGs)."""

import math
import os
import struct
import subprocess
import sys
import zlib


def write_png(path: str, width: int, height: int, rgba: bytes) -> None:
    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    raw = bytearray()
    stride = width * 4
    for y in range(height):
        raw.append(0)
        raw.extend(rgba[y * stride : (y + 1) * stride])

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", zlib.compress(bytes(raw), 6))
        + chunk(b"IEND", b"")
    )
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(png)


def render_icon(size: int) -> bytes:
    buf = bytearray(size * size * 4)
    center = (size - 1) * 0.5
    radius = size * 0.455

    for y in range(size):
        fy = (y - center) / radius
        ny4 = abs(fy) ** 4.2
        for x in range(size):
            fx = (x - center) / radius
            r4 = abs(fx) ** 4.2 + ny4
            if r4 > 1.0:
                continue

            edge_aa = max(0.0, min(1.0, (1.0 - r4) * (size * 0.22)))
            radial = min(1.0, math.hypot(fx, fy))

            # Studio Obsidian deep charcoal base
            r = 13.0 + (1.0 - radial) * 20.0
            g = 15.0 + (1.0 - radial) * 24.0
            b = 22.0 + (1.0 - radial) * 38.0

            # Jewel-tone Chroma rim
            if r4 > 0.84:
                rim = min(1.0, (r4 - 0.84) / 0.16)
                ang = math.atan2(fy, fx)
                c_mix = math.cos(ang) * 0.5 + 0.5
                r += rim * (56.0 * c_mix + 236.0 * (1.0 - c_mix)) * 0.85
                g += rim * (189.0 * c_mix + 72.0 * (1.0 - c_mix)) * 0.85
                b += rim * (248.0 * c_mix + 153.0 * (1.0 - c_mix)) * 0.85

            # Play-prism glyph in center
            px = (fx + 0.34) / 0.78
            py = fy / 0.56
            if 0.0 <= px <= 1.0 and abs(py) <= (1.0 - px * 0.90):
                v = max(0.0, min(1.0, py * 0.5 + 0.5))
                r = 56.0 * (1.0 - px) + 168.0 * px + 75.0 * v * px
                g = 195.0 * (1.0 - px) + 82.0 * px + 65.0 * (1.0 - v)
                b = 250.0 * (1.0 - px * 0.62) + 35.0 * (1.0 - v)

            idx = (y * size + x) * 4
            buf[idx] = int(max(0.0, min(255.0, r)))
            buf[idx + 1] = int(max(0.0, min(255.0, g)))
            buf[idx + 2] = int(max(0.0, min(255.0, b)))
            buf[idx + 3] = int(edge_aa * 255.0)

    return bytes(buf)


def main() -> None:
    out_dir = sys.argv[1] if len(sys.argv) > 1 else "dist/icons"
    os.makedirs(out_dir, exist_ok=True)

    master_512 = os.path.join(out_dir, "foreplay_512.png")
    write_png(master_512, 512, 512, render_icon(512))
    write_png(os.path.join(out_dir, "foreplay_256.png"), 256, 256, render_icon(256))
    write_png(os.path.join(out_dir, "foreplay_128.png"), 128, 128, render_icon(128))
    write_png(os.path.join(out_dir, "foreplay_64.png"), 64, 64, render_icon(64))

    # Build macOS .icns if on macOS with sips + iconutil
    if sys.platform == "darwin":
        iconset = os.path.join(out_dir, "Foreplay.iconset")
        os.makedirs(iconset, exist_ok=True)
        write_png(os.path.join(iconset, "icon_512x512@2x.png"), 1024, 1024, render_icon(1024))
        for pt in [16, 32, 128, 256, 512]:
            p1 = os.path.join(iconset, f"icon_{pt}x{pt}.png")
            p2 = os.path.join(iconset, f"icon_{pt}x{pt}@2x.png")
            subprocess.run(
                ["sips", "-z", str(pt), str(pt), os.path.join(iconset, "icon_512x512@2x.png"), "--out", p1],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            if pt < 512:
                subprocess.run(
                    ["sips", "-z", str(pt * 2), str(pt * 2), os.path.join(iconset, "icon_512x512@2x.png"), "--out", p2],
                    check=True,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                )
        icns_path = os.path.join(out_dir, "Foreplay.icns")
        subprocess.run(["iconutil", "-c", "icns", iconset, "-o", icns_path], check=True)
        print(f"Generated macOS Retina icon: {icns_path}")

    print(f"Generated Linux hicolor PNG icons in: {out_dir}")


if __name__ == "__main__":
    main()
