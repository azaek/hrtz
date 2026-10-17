"""Build every app icon from the emblem.

    python -I branding/build-icons.py        (from the app folder)

Writes branding/app-icon.svg (the source of truth: the emblem on the dark plate), regenerates
src-tauri/icons with `tauri icon`, then replaces icon.ico and 32x32.png with versions whose small
sizes are tuned. Below about 32 px of emblem height, the seam between the two halves drops under a
pixel and closes up, so those sizes push the halves apart until the seam is about a pixel wide.
Sizes from 64 px up are exactly as drawn.
"""
import math
import shutil
import struct
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
APP = HERE.parent
ICONS = APP / "src-tauri" / "icons"
WORK = HERE / ".build"

# The emblem, 226 x 247: the left button (tall) and the right (short).
W, H, SEAM = 226, 247, 8.4  # the seam is 8.4 units at its narrowest, at the top
LEFT = (
    "M0 112.091C0 59.6577 35.7123 15.565 84.1404 2.81008C95.2232 -0.108917 100.765 -1.56842 105.876 2.53566"
    "C110.987 6.63973 110.723 13.2744 110.195 26.5436L102.447 221.126C101.977 232.934 101.742 238.838 97.968 242.465"
    "C94.1942 246.091 88.2853 246.091 76.4677 246.091H29.7025C19.719 246.091 14.7272 246.091 10.8658 244.262"
    "C6.90057 242.384 3.70768 239.191 1.82925 235.226C0 231.364 0 226.372 0 216.389V112.091Z"
)
RIGHT = (
    "M118.629 102.064C118.082 88.7644 117.809 82.1144 122.96 78.0039C128.11 73.8934 133.629 75.3895 144.666 78.3815"
    "C192.383 91.3162 226 135.728 226 188.091V216.389C226 226.372 226 231.364 224.171 235.226"
    "C222.292 239.191 219.099 242.384 215.134 244.262C211.273 246.091 206.281 246.091 196.297 246.091H149.501"
    "C137.697 246.091 131.795 246.091 128.022 242.471C124.249 238.85 124.007 232.953 123.523 221.159L118.629 102.064Z"
)

CANVAS, MARK = 1024, 600  # the emblem is 600 of the plate's 1024, about 59%
ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]


def squircle(cx: float, cy: float, r: float, n: float = 5, steps: int = 180) -> str:
    pts = []
    for i in range(steps):
        t = i / steps * 2 * math.pi
        c, s = math.cos(t), math.sin(t)
        x = cx + r * math.copysign(abs(c) ** (2 / n), c)
        y = cy + r * math.copysign(abs(s) ** (2 / n), s)
        pts.append(f"{'M' if i == 0 else 'L'}{x:.1f} {y:.1f}")
    return "".join(pts) + "Z"


def spread_for(mark_px: float) -> float:
    """How far to push each half out, in emblem units, so the seam stays about a pixel wide."""
    if mark_px > 32:
        return 0.0
    return max(0.0, (1.1 * H / mark_px - SEAM) / 2)


def app_icon_svg(spread: float = 0.0, note: bool = False) -> str:
    s = MARK / H
    tx, ty = CANVAS / 2 - W * s / 2, CANVAS / 2 - H * s / 2
    comment = (
        "\n  <!-- Source of truth for the app icon: the hrtz emblem on the dark plate. Rebuild every size\n"
        "       with `python -I branding/build-icons.py`. The plate belongs to the app icon only;\n"
        "       everywhere else the emblem stands alone (branding/emblem.svg). -->"
        if note
        else ""
    )
    return f"""<svg width="{CANVAS}" height="{CANVAS}" viewBox="0 0 {CANVAS} {CANVAS}" xmlns="http://www.w3.org/2000/svg">{comment}
  <defs>
    <linearGradient id="plate" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#383838"/>
      <stop offset="1" stop-color="#1d1d1d"/>
    </linearGradient>
  </defs>
  <path d="{squircle(512, 512, 448)}" fill="url(#plate)"/>
  <path d="{squircle(512, 512, 444)}" fill="none" stroke="#ffffff" stroke-opacity="0.10" stroke-width="8"/>
  <g transform="translate({tx:.2f} {ty:.2f}) scale({s:.5f})" fill="#ffffff" fill-rule="evenodd">
    <path transform="translate({-spread:.2f} 0)" d="{LEFT}"/>
    <path transform="translate({spread:.2f} 0)" d="{RIGHT}"/>
  </g>
</svg>
"""


def emblem_svg() -> str:
    return f"""<svg width="{W}" height="{H}" viewBox="0 0 {W} {H}" fill="none" xmlns="http://www.w3.org/2000/svg">
  <path fill-rule="evenodd" clip-rule="evenodd" d="{RIGHT}" fill="black"/>
  <path fill-rule="evenodd" clip-rule="evenodd" d="{LEFT}" fill="black"/>
</svg>
"""


def tauri_icon(src: Path, out: Path, png: int | None = None) -> None:
    cmd = ["pnpm", "tauri", "icon", str(src), "-o", str(out)]
    if png:
        cmd += ["-p", str(png)]
    subprocess.run(cmd, cwd=APP, check=True, shell=sys.platform == "win32", stdout=subprocess.DEVNULL)


def write_ico(frames: list[tuple[int, bytes]], path: Path) -> None:
    """An .ico with one PNG-encoded image per size (supported since Windows Vista)."""
    header = struct.pack("<HHH", 0, 1, len(frames))
    offset = 6 + 16 * len(frames)
    entries, blobs = b"", b""
    for size, data in frames:
        dim = 0 if size >= 256 else size
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
        blobs += data
        offset += len(data)
    path.write_bytes(header + entries + blobs)


def main() -> None:
    (HERE / "app-icon.svg").write_text(app_icon_svg(note=True), encoding="utf-8", newline="\n")
    (HERE / "emblem.svg").write_text(emblem_svg(), encoding="utf-8", newline="\n")

    # Every platform's set from the drawn icon.
    tauri_icon(HERE / "app-icon.svg", ICONS)
    for extra in ("android", "ios"):
        shutil.rmtree(ICONS / extra, ignore_errors=True)

    # Then each .ico frame on its own, tuned where it is small.
    shutil.rmtree(WORK, ignore_errors=True)
    WORK.mkdir()
    frames = []
    for size in ICO_SIZES:
        src = WORK / f"icon-{size}.svg"
        src.write_text(app_icon_svg(spread_for(size * MARK / CANVAS)), encoding="utf-8")
        tauri_icon(src, WORK / str(size), png=size)
        frames.append((size, (WORK / str(size) / f"{size}x{size}.png").read_bytes()))
    write_ico(frames, ICONS / "icon.ico")
    shutil.copyfile(WORK / "32" / "32x32.png", ICONS / "32x32.png")
    shutil.rmtree(WORK)
    print(f"icons written to {ICONS}")


if __name__ == "__main__":
    main()
