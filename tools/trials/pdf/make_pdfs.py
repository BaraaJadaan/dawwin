"""Trial 2 of Dawwin 1.8.0: PDFs of made-up Arabic, made the ways real ones are made.

    python make_pdfs.py <out folder>

Writes `expected.json` (the text in reading order), `page.html`, and the PDFs this script can make
itself: Chrome's "print to PDF" and a screenshot of the same page as a scan (both through headless
Chrome in a throwaway profile), an old file stored in visual order with presentation forms (what
generators did before shaping, through arabic_reshaper and python-bidi), and a naive one in logical
order with unjoined letters (ReportLab without shaping). Word's own "Save as PDF" and "Microsoft Print
to PDF" come from make_word.ps1 and make_print.ps1. Needs reportlab, arabic-reshaper, python-bidi<0.5.
"""

import json
import subprocess
import sys
from pathlib import Path

TITLE = "تجربة قراءة PDF"
PARAGRAPHS = [
    "مرحبًا، هذه فقرة مختلقة للتجربة: يعمل برنامج Dawwin على الحاسوب نفسه (offline)، ويقرأ ملفات PDF بالعربية والإنجليزية. هل يُقرأ «النص» بالترتيب الصحيح؟ نعم! السعر 25.5% من 1,200 ريال في عام 2026.",
    "الجملةُ الثانيةُ مشكولةٌ: بِسْمِ اللهِ، ثُمَّ كلمة English في الوسط؛ ثم رقم ١٢٣ بالأرقام العربية.",
    "An English paragraph with an Arabic quote inside: «مرحبًا بالعالم» and a number, 3.14, at the end.",
    "البند الأول: Windows 11، والبند الثاني: النسخة 1.8.0 من البرنامج.",
]
CHROME = r"C:\Program Files\Google\Chrome\Application\chrome.exe"
FONT = r"C:\Windows\Fonts\arial.ttf"


def is_arabic(text):
    return any("\u0600" <= c <= "\u06ff" for c in text)


def page_html():
    paras = "\n".join(
        f'<p dir="{"ltr" if not is_arabic(p.split(":")[0]) else "rtl"}">{p}</p>' for p in PARAGRAPHS
    )
    return f"""<!doctype html><html lang="ar" dir="rtl"><head><meta charset="utf-8"><title>{TITLE}</title>
<style>body{{font:18px "Segoe UI";margin:40px;max-width:640px}} h1{{font-size:26px}}</style></head>
<body><h1>{TITLE}</h1>
{paras}
</body></html>"""


def chrome(out, *args):
    profile = out / "chrome-profile"
    subprocess.run([CHROME, "--headless=new", "--disable-gpu", f"--user-data-dir={profile}", *args], check=True, timeout=120)


def lines(text, words=7):
    w = text.split()
    return [" ".join(w[i : i + words]) for i in range(0, len(w), words)]


def reportlab_pdf(path, visual):
    from reportlab.pdfbase import pdfmetrics
    from reportlab.pdfbase.ttfonts import TTFont
    from reportlab.pdfgen import canvas

    pdfmetrics.registerFont(TTFont("Arial", FONT))
    c = canvas.Canvas(str(path))
    c.setTitle(TITLE)
    y = 780
    for block in [TITLE, *PARAGRAPHS]:
        rtl = is_arabic(block.split(":")[0])
        for line in lines(block):
            if visual:
                import arabic_reshaper
                from bidi.algorithm import get_display

                shown = get_display(arabic_reshaper.reshape(line), base_dir="R" if rtl else "L")
            else:
                shown = line
            c.setFont("Arial", 13)
            if rtl:
                c.drawRightString(540, y, shown)
            else:
                c.drawString(56, y, shown)
            y -= 20
        y -= 12
    c.save()


def scan_pdf(out):
    """The page as a picture only, as a scanner makes it."""
    from reportlab.lib.utils import ImageReader
    from reportlab.pdfgen import canvas

    png = out / "scan.png"
    chrome(out, f"--screenshot={png}", "--window-size=760,900", "--force-device-scale-factor=2", "--hide-scrollbars", (out / "page.html").as_uri())
    c = canvas.Canvas(str(out / "scan.pdf"))
    c.drawImage(ImageReader(str(png)), 20, 60, width=555, height=657)
    c.save()


def main():
    out = Path(sys.argv[1]).resolve()
    out.mkdir(parents=True, exist_ok=True)
    (out / "expected.json").write_text(json.dumps({"title": TITLE, "paragraphs": PARAGRAPHS}, ensure_ascii=False, indent=1), encoding="utf-8")
    (out / "page.html").write_text(page_html(), encoding="utf-8")
    chrome(out, "--no-pdf-header-footer", f"--print-to-pdf={out / 'chrome.pdf'}", (out / "page.html").as_uri())
    reportlab_pdf(out / "visual-order.pdf", visual=True)
    reportlab_pdf(out / "naive-logical.pdf", visual=False)
    scan_pdf(out)
    print("made:", ", ".join(sorted(p.name for p in out.glob("*.pdf"))))


if __name__ == "__main__":
    main()
