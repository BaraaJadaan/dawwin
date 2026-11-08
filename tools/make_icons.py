"""Draw the app icon (an ink drop holding a sound wave) at every size Tauri needs."""
import math
import sys
from pathlib import Path

from PIL import Image, ImageDraw

out = Path(sys.argv[1])
S = 1024
K = S / 120  # the design is on a 120 x 120 grid
NAVY = (30, 58, 95, 255)
WHITE = (255, 255, 255, 255)


def bezier(p0, p1, p2, p3, n=80):
    pts = []
    for i in range(n + 1):
        t = i / n
        u = 1 - t
        pts.append(tuple(u**3 * a + 3 * u**2 * t * b + 3 * u * t**2 * c + t**3 * d for a, b, c, d in zip(p0, p1, p2, p3)))
    return pts


def draw():
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([4 * K, 4 * K, 116 * K, 116 * K], radius=28 * K, fill=NAVY)
    # the drop: two curves from the tip into a round belly
    left = bezier((60, 16), (60, 16), (30, 52), (30, 72))
    right = bezier((90, 72), (90, 52), (60, 16), (60, 16))
    belly = [(60 - 30 * math.cos(i * math.pi / 80), 72 + 30 * math.sin(i * math.pi / 80)) for i in range(81)]
    d.polygon([(x * K, y * K) for x, y in left + belly + right], fill=WHITE)
    # the wave
    for x, y0, y1 in [(46, 70, 80), (55, 62, 88), (64, 66, 84), (73, 70, 80)]:
        d.rounded_rectangle([(x - 3) * K, (y0 - 3) * K, (x + 3) * K, (y1 + 3) * K], radius=3 * K, fill=NAVY)
    return img


img = draw()
img.resize((512, 512), Image.LANCZOS).save(out / "icon.png")
img.resize((128, 128), Image.LANCZOS).save(out / "128x128.png")
img.resize((256, 256), Image.LANCZOS).save(out / "128x128@2x.png")
img.resize((32, 32), Image.LANCZOS).save(out / "32x32.png")
img.save(out / "icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
print("icons written to", out)
