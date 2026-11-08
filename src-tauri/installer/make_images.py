"""Renders the NSIS installer artwork (header.bmp, sidebar.bmp).

Run from anywhere:  python src-tauri/installer/make_images.py
Needs Pillow and the Segoe UI fonts that ship with Windows.
"""
import os
from PIL import Image, ImageDraw, ImageFilter, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
ICON = os.path.join(HERE, "..", "icons", "icon.png")
FONTS = r"C:\Windows\Fonts"
S = 4  # supersample factor

GREEN = (142, 230, 169)
BLUE = (97, 203, 246)
TOP = (37, 42, 53)
BOTTOM = (13, 15, 19)


def font(name, px):
    return ImageFont.truetype(os.path.join(FONTS, name), int(px * S))


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def background(w, h, glows):
    im = Image.new("RGB", (w, h))
    px = im.load()
    for y in range(h):
        for x in range(w):
            px[x, y] = lerp(TOP, BOTTOM, min(1, (y / h) * 0.85 + (x / w) * 0.15))
    for cx, cy, r, color, alpha in glows:
        layer = Image.new("RGB", (w, h), color)
        mask = Image.new("L", (w, h), 0)
        ImageDraw.Draw(mask).ellipse((cx - r, cy - r, cx + r, cy + r), fill=alpha)
        mask = mask.filter(ImageFilter.GaussianBlur(r * 0.55))
        im = Image.composite(layer, im, mask)
    return im


def gradient_fill(im, xy, text, fnt):
    """Draws text filled with the green→blue brand gradient."""
    mask = Image.new("L", im.size, 0)
    ImageDraw.Draw(mask).text(xy, text, font=fnt, fill=255)
    x0, _, x1, _ = mask.getbbox()
    grad = Image.new("RGB", im.size)
    g = ImageDraw.Draw(grad)
    for x in range(im.size[0]):
        t = min(1, max(0, (x - x0) / max(1, x1 - x0)))
        g.line([(x, 0), (x, im.size[1])], fill=lerp(GREEN, BLUE, t))
    im.paste(grad, (0, 0), mask)


def gradient_bar(im, box, radius):
    x0, y0, x1, y1 = box
    mask = Image.new("L", im.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle(box, radius=radius, fill=255)
    grad = Image.new("RGB", im.size)
    g = ImageDraw.Draw(grad)
    for x in range(int(x0), int(x1) + 1):
        g.line([(x, y0), (x, y1)], fill=lerp(GREEN, BLUE, (x - x0) / max(1, x1 - x0)))
    im.paste(grad, (0, 0), mask)


def blend(color, alpha, base=BOTTOM):
    return lerp(base, color, alpha)


def paste_icon(im, x, y, size):
    icon = Image.open(ICON).convert("RGBA").resize((size, size), Image.LANCZOS)
    im.paste(icon, (int(x), int(y)), icon)


def finish(im, w, h, name):
    out = im.resize((w, h), Image.LANCZOS).convert("RGB")
    out.save(os.path.join(HERE, name), "BMP")
    print("wrote", name, out.size)


def sidebar():
    w, h = 164, 314
    W, H = w * S, h * S
    im = background(W, H, [
        (int(W * 0.1), int(H * 0.12), int(W * 0.75), GREEN, 46),
        (int(W * 0.95), int(H * 0.92), int(W * 0.8), BLUE, 40),
    ])
    d = ImageDraw.Draw(im)
    m = 18 * S

    paste_icon(im, m - 4 * S, 24 * S, 52 * S)

    d.text((m, 86 * S), "Top", font=font("seguibl.ttf", 27), fill=(255, 255, 255))
    tw = d.textlength("Top", font=font("seguibl.ttf", 27))
    gradient_fill(im, (m + tw, 86 * S), "Note", font("seguibl.ttf", 27))

    tag = font("segoeui.ttf", 11.5)
    for i, line in enumerate(["Synced lyrics that float", "above everything."]):
        d.text((m, (124 + i * 16) * S), line, font=tag, fill=blend((255, 255, 255), 0.66, (30, 34, 43)))

    # A tiny lyric card, like the app itself.
    card = (m - 6 * S, 186 * S, W - m + 6 * S, 286 * S)
    overlay = Image.new("RGBA", im.size, (0, 0, 0, 0))
    od = ImageDraw.Draw(overlay)
    od.rounded_rectangle(card, radius=10 * S, fill=(255, 255, 255, 14), outline=(255, 255, 255, 26), width=S)
    im.paste(Image.alpha_composite(im.convert("RGBA"), overlay).convert("RGB"))
    d = ImageDraw.Draw(im)

    dim = font("seguisb.ttf", 10.5)
    d.text((m + 4 * S, 198 * S), "and the city hums along", font=dim, fill=(112, 117, 128))
    gradient_fill(im, (m + 4 * S, 216 * S), "I can hear it on top", font("segoeuib.ttf", 12.5))
    d.text((m + 4 * S, 238 * S), "of every window I own", font=dim, fill=(150, 155, 165))

    bar_y = 266 * S
    d.rounded_rectangle((m + 4 * S, bar_y, W - m - 4 * S, bar_y + 3 * S), radius=2 * S, fill=(52, 57, 68))
    gradient_bar(im, (m + 4 * S, bar_y, m + 4 * S + int((W - 2 * m - 8 * S) * 0.58), bar_y + 3 * S), 2 * S)

    finish(im, w, h, "sidebar.bmp")


def header():
    w, h = 150, 57
    W, H = w * S, h * S
    im = background(W, H, [
        (int(W * 0.05), int(H * 0.2), int(W * 0.45), GREEN, 40),
        (int(W * 1.0), int(H * 1.0), int(W * 0.45), BLUE, 34),
    ])
    d = ImageDraw.Draw(im)
    paste_icon(im, 12 * S, 11 * S, 35 * S)
    f = font("seguibl.ttf", 18)
    y = 15 * S
    d.text((52 * S, y), "Top", font=f, fill=(255, 255, 255))
    gradient_fill(im, (52 * S + d.textlength("Top", font=f), y), "Note", f)
    finish(im, w, h, "header.bmp")


sidebar()
header()
