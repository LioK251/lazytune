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

# A dark edge keeps the native toolbar glyphs visible on both Windows themes.
taskbar = out / 'taskbar'
taskbar.mkdir(exist_ok=True)
for name in ('like', 'liked', 'previous', 'play', 'pause', 'next'):
    scale = 8
    toolbar = Image.new('RGBA', (192, 192))
    td = ImageDraw.Draw(toolbar)

    def shape(coords, filled=True):
        coords = [(x * scale, y * scale) for x, y in coords]
        if filled:
            td.polygon(coords, fill='#DDDDDD')
            td.line(coords + [coords[0]], fill='#252525', width=5, joint='curve')
        else:
            td.line(coords, fill='#252525', width=20, joint='curve')
            td.line(coords, fill='#DDDDDD', width=13, joint='curve')

    if name in ('like', 'liked'):
        points = []
        curve((12, 6), (5, -1), (-1, 8), (5, 13))
        curve((5, 13), (5, 13), (12, 20), (12, 20))
        curve((12, 20), (12, 20), (19, 13), (19, 13))
        curve((19, 13), (25, 8), (19, -1), (12, 6))
        shape([(x / scale, y / scale) for x, y in points], name == 'liked')
    elif name == 'play':
        shape([(8, 4), (20, 12), (8, 20)])
    elif name == 'pause':
        shape([(6, 4), (10, 4), (10, 20), (6, 20)])
        shape([(14, 4), (18, 4), (18, 20), (14, 20)])
    else:
        coords = [(5, 5), (16, 12), (5, 19)]
        bar = [(18, 5), (20, 5), (20, 19), (18, 19)]
        if name == 'previous':
            coords = [(24 - x, y) for x, y in coords]
            bar = [(24 - x, y) for x, y in bar]
        shape(coords)
        shape(bar)
    toolbar.resize((48, 48), Image.Resampling.LANCZOS).save(taskbar / f'{name}.png')
print('Generated Windows taskbar icons')
