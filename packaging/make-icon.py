"""Render assets/icon.ico from the shapes in assets/icon.svg.

The SVG is five primitives (rounded square, an L-shaped path, a circle and
four rounded lines), so this redraws them with Pillow instead of pulling in
an SVG rasterizer. Run once after the SVG changes, commit the .ico:

    py packaging/make-icon.py

Sizes match what Windows asks for on the desktop, in Explorer, in the taskbar
and in Programs and Features.
"""

from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "assets" / "icon.ico"
SIZES = [16, 24, 32, 48, 64, 128, 256]

BG = "#c9f6f2"
FG = "#006d68"
SCALE = 4  # draw at 1024 px, then downsample for smooth edges
S = 256 * SCALE
W = 20 * SCALE  # stroke width


def line(draw: ImageDraw.ImageDraw, x1: int, y1: int, x2: int, y2: int) -> None:
    """A stroke with round caps, like stroke-linecap="round"."""
    r = W // 2
    draw.line([(x1 * SCALE, y1 * SCALE), (x2 * SCALE, y2 * SCALE)], fill=FG, width=W)
    for x, y in ((x1, y1), (x2, y2)):
        cx, cy = x * SCALE, y * SCALE
        draw.ellipse([cx - r, cy - r, cx + r, cy + r], fill=FG)


def render() -> Image.Image:
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    draw.rounded_rectangle([0, 0, S - 1, S - 1], radius=56 * SCALE, fill=BG)
    # path: M64 116V96 M64 116h128 M192 116V72
    line(draw, 64, 116, 64, 96)
    line(draw, 64, 116, 192, 116)
    line(draw, 192, 116, 192, 72)
    # circle cx=192 cy=46 r=20
    r = 20 * SCALE
    draw.ellipse([192 * SCALE - r, 46 * SCALE - r, 192 * SCALE + r, 46 * SCALE + r], fill=FG)
    # baseline and three bars
    line(draw, 60, 163, 196, 163)
    line(draw, 76, 187, 76, 163)
    line(draw, 128, 194, 128, 163)
    line(draw, 180, 187, 180, 163)
    return img


def main() -> None:
    base = render()
    frames = [base.resize((s, s), Image.LANCZOS) for s in SIZES]
    frames[-1].save(OUT, format="ICO", sizes=[(s, s) for s in SIZES], append_images=frames[:-1])
    print(f"wrote {OUT} ({OUT.stat().st_size} bytes, sizes {SIZES})")


if __name__ == "__main__":
    main()
