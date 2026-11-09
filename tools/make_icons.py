"""Draw the app icon (a sound wave whose middle bar is a pen nib) at every size Tauri needs."""
import sys
from pathlib import Path

from PIL import Image, ImageDraw

out = Path(sys.argv[1])
S = 1024
K = S / 120  # the design is on a 120 x 120 grid
TEAL = (15, 118, 110, 255)  # --accent in ui/style.css
BARS = (167, 230, 219, 255)
WHITE = (255, 255, 255, 255)


def bezier(p0, p1, p2, p3, n=80):
    pts = []
    for i in range(n + 1):
        t = i / n
        u = 1 - t
        pts.append(tuple(u**3 * a + 3 * u**2 * t * b + 3 * u * t**2 * c + t**3 * d for a, b, c, d in zip(p0, p1, p2, p3)))
    return pts


def scaled(pts):
    return [(x * K, y * K) for x, y in pts]


def draw():
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([4 * K, 4 * K, 116 * K, 116 * K], radius=26 * K, fill=TEAL)
    # the sound wave on both sides of the nib
    for x, y0, y1 in [(22, 50, 70), (36, 39, 81), (76, 39, 81), (90, 50, 70)]:
        d.rounded_rectangle([x * K, y0 * K, (x + 8) * K, y1 * K], radius=4 * K, fill=BARS)
    # the nib: from its tip up the left side to the stem, and down the right side again
    left = bezier((60, 104), (54.5, 94), (49, 81), (49, 70)) + bezier((49, 70), (49, 62), (53.5, 56), (55, 50))
    right = bezier((65, 50), (66.5, 56), (71, 62), (71, 70)) + bezier((71, 70), (71, 81), (65.5, 94), (60, 104))
    d.polygon(scaled(left + [(55, 21), (65, 21)] + right), fill=WHITE)
    d.ellipse([55 * K, 16 * K, 65 * K, 26 * K], fill=WHITE)
    # the slit and the hole
    d.rounded_rectangle([58.8 * K, 72 * K, 61.2 * K, 102 * K], radius=1.2 * K, fill=TEAL)
    d.ellipse([56.4 * K, 68.4 * K, 63.6 * K, 75.6 * K], fill=TEAL)
    return img


# Small sizes blur when scaled down, so they are drawn on their own pixel grid, in pixels.
# bars: (x0, x1, y0, y1). The nib: its stem's sides, how wide it gets, and where it starts and ends.
SMALL = {
    16: dict(radius=3.5, bars=[(3, 4, 7, 9), (5, 6, 5, 11), (10, 11, 5, 11), (12, 13, 7, 9)],
             stem=(7, 9), belly=(6.5, 9.5), top=2, shoulder=7, widest=9.5, tip=14.5, hole=None),
    24: dict(radius=5, bars=[(4, 6, 10, 14), (7, 9, 8, 16), (15, 17, 8, 16), (18, 20, 10, 14)],
             stem=(11, 13), belly=(10, 14), top=3, shoulder=10, widest=14, tip=21.5, hole=None),
    32: dict(radius=7, bars=[(6, 8, 13, 19), (10, 12, 10, 22), (20, 22, 10, 22), (24, 26, 13, 19)],
             stem=(15, 17), belly=(13, 19), top=4, shoulder=13, widest=19, tip=29, hole=(15, 17, 18, 20)),
}


def draw_small(size):
    p = SMALL[size]
    f = 16  # drawn larger and averaged down: straight edges stay sharp, slanted ones get smooth
    im = Image.new("RGBA", (size * f, size * f), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    d.rounded_rectangle([0, 0, size * f - 1, size * f - 1], radius=p["radius"] * f, fill=TEAL)
    for x0, x1, y0, y1 in p["bars"]:
        d.rectangle([x0 * f, y0 * f, x1 * f - 1, y1 * f - 1], fill=BARS)
    (sl, sr), (bl, br) = p["stem"], p["belly"]
    nib = [(sl, p["top"]), (sr, p["top"]), (sr, p["shoulder"]), (br, p["widest"]), (size / 2, p["tip"]), (bl, p["widest"]), (sl, p["shoulder"])]
    d.polygon([(x * f, y * f) for x, y in nib], fill=WHITE)
    if p["hole"]:
        x0, x1, y0, y1 = p["hole"]
        d.rectangle([x0 * f, y0 * f, x1 * f - 1, y1 * f - 1], fill=TEAL)
    return im.resize((size, size), Image.BOX)


img = draw()
small = {size: draw_small(size) for size in SMALL}
img.resize((512, 512), Image.LANCZOS).save(out / "icon.png")
img.resize((128, 128), Image.LANCZOS).save(out / "128x128.png")
img.resize((256, 256), Image.LANCZOS).save(out / "128x128@2x.png")
small[32].save(out / "32x32.png")
# every size is given, or Pillow scales the missing ones from the last image given
frames = list(small.values()) + [img.resize((s, s), Image.LANCZOS) for s in (48, 64, 128, 256)]
img.save(out / "icon.ico", sizes=[f.size for f in frames], append_images=frames)
img.resize((128, 128), Image.LANCZOS).save(Path(__file__).resolve().parent.parent / "ui" / "icon.png")
print("icons written to", out)
