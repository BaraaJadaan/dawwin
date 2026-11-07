"""Draw the app icon (a speech wave turning into lines of text) at every size Tauri needs."""
import sys
from pathlib import Path

from PIL import Image, ImageDraw

out = Path(sys.argv[1])
S = 1024


def draw():
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    # teal gradient tile
    grad = Image.new("RGBA", (S, S))
    gd = ImageDraw.Draw(grad)
    for y in range(S):
        t = y / S
        c = (int(20 + 10 * t), int(150 - 40 * t), int(140 - 30 * t), 255)
        gd.line([(0, y), (S, y)], fill=c)
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle([40, 40, S - 40, S - 40], radius=220, fill=255)
    img.paste(grad, (0, 0), mask)
    d = ImageDraw.Draw(img)
    white = (255, 255, 255, 255)
    soft = (255, 255, 255, 170)
    # wave bars on the right, text lines on the left (the transcript reads right to left)
    bars = [120, 260, 400, 300, 180]
    x = 610
    for h in bars:
        d.rounded_rectangle([x, 512 - h // 2, x + 52, 512 + h // 2], radius=26, fill=white)
        x += 76
    for i, w in enumerate([330, 250, 300]):
        y = 372 + i * 110
        d.rounded_rectangle([520 - w, y, 520, y + 52], radius=26, fill=soft if i else white)
    return img


img = draw()
img.resize((512, 512), Image.LANCZOS).save(out / "icon.png")
img.resize((128, 128), Image.LANCZOS).save(out / "128x128.png")
img.resize((256, 256), Image.LANCZOS).save(out / "128x128@2x.png")
img.resize((32, 32), Image.LANCZOS).save(out / "32x32.png")
img.save(out / "icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
print("icons written to", out)
