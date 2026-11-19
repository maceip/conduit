# Cuts round corners into docs/social-preview.jpg => docs/social-preview-rounded.png.
# Needs Pillow: python3 -m pip install Pillow
from pathlib import Path
from PIL import Image, ImageDraw

docs = Path(__file__).resolve().parents[2] / "docs"
im = Image.open(docs / "social-preview.jpg").convert("RGBA")
w, h = im.size
r, s = 36, 4  # radius in px; draw the mask 4x and scale down for smooth edges
mask = Image.new("L", (w * s, h * s), 0)
ImageDraw.Draw(mask).rounded_rectangle((0, 0, w * s - 1, h * s - 1), r * s, fill=255)
im.putalpha(mask.resize((w, h), Image.LANCZOS))
im.save(docs / "social-preview-rounded.png", optimize=True)
