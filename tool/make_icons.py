"""Rasterize the existing web/favicon.svg geometry into platform icon sizes."""
from pathlib import Path
from PIL import Image, ImageDraw

root = Path(__file__).resolve().parents[1]
scale = 16
im = Image.new('RGBA', (64*scale, 64*scale))
draw = ImageDraw.Draw(im)
draw.rounded_rectangle((0, 0, 64*scale-1, 64*scale-1), radius=20*scale, fill='#8070ad')
curves = [((32,32),(26,19),(12,19),(12,32)),
          ((12,32),(12,45),(26,45),(32,32)),
          ((32,32),(38,19),(52,19),(52,32)),
          ((52,32),(52,45),(38,45),(32,32))]
points = []
for a,b,c,d in curves:
    for i in range(101):
        t = i/100
        points.append(tuple(scale*((1-t)**3*a[k]+3*(1-t)**2*t*b[k]+3*(1-t)*t*t*c[k]+t**3*d[k]) for k in (0,1)))
draw.line(points, fill='white', width=4*scale, joint='curve')
icon = im.resize((256,256), Image.Resampling.LANCZOS)
icon.save(root/'windows/runner/resources/app_icon.ico', sizes=[(s,s) for s in (16,24,32,48,64,128,256)])
for name,size in [('Icon-192.png',192),('Icon-512.png',512),('Icon-maskable-192.png',192),('Icon-maskable-512.png',512)]:
    im.resize((size,size), Image.Resampling.LANCZOS).save(root/'web/icons'/name)
