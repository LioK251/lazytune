"""Generate the original lazytune mark and platform icons; requires Pillow."""
from pathlib import Path
from PIL import Image, ImageDraw

root = Path(__file__).resolve().parents[1]
out = root / 'desktop' / 'icons'
out.mkdir(parents=True, exist_ok=True)
size = 2048
im = Image.new('RGBA', (size, size), (0, 0, 0, 0))
d = ImageDraw.Draw(im)
scale = size / 24
def rect(box, radius, color):
    d.rounded_rectangle(tuple(v * scale for v in box), radius=radius * scale, fill=color)

# Squared, softly rounded proportions echo the keys in the companion app.
rect((5, 4, 8.5, 20), .8, '#DDDDDD')
rect((5, 16.5, 15.8, 20), .8, '#DDDDDD')
points = []
def curve(a, b, c, e):
    for i in range(25):
        t = i / 24
        points.append(tuple(scale * ((1-t)**3*a[j] + 3*(1-t)**2*t*b[j] + 3*(1-t)*t*t*c[j] + t**3*e[j]) for j in (0,1)))
curve((12, 5.7), (12, 5.05), (12.6, 4.75), (13.15, 5.1))
curve((13.15, 5.1), (13.15, 5.1), (19.45, 9), (19.45, 9))
curve((19.45, 9), (20, 9.34), (20, 10.06), (19.45, 10.4))
curve((19.45, 10.4), (19.45, 10.4), (13.15, 14.3), (13.15, 14.3))
curve((13.15, 14.3), (12.6, 14.65), (12, 14.35), (12, 13.7))
d.polygon(points, fill='#ADADAD')
icon = im.resize((1024, 1024), Image.Resampling.LANCZOS)
icon.save(out / 'lazytune-1024.png')
icon.resize((256, 256), Image.Resampling.LANCZOS).save(out / 'icon.png')
icon.save(out / 'icon.ico', sizes=[(16,16),(24,24),(32,32),(48,48),(64,64),(128,128),(256,256)])
icon.save(out / 'icon.icns')
(out / 'mark.svg').write_text('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><title>lazytune</title><g fill="#DDDDDD"><rect x="5" y="4" width="3.5" height="16" rx=".8"/><rect x="5" y="16.5" width="10.8" height="3.5" rx=".8"/></g><path d="M12 5.7C12 5.05 12.6 4.75 13.15 5.1L19.45 9C20 9.34 20 10.06 19.45 10.4L13.15 14.3C12.6 14.65 12 14.35 12 13.7Z" fill="#ADADAD"/></svg>', encoding='utf-8')
print('Generated desktop/icons/lazytune-1024.png and platform icons')
