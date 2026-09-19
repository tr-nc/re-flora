"""Read-only source analysis; diagnostic contact sheet preserves every source pixel."""
import hashlib
import json
from pathlib import Path
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent
SOURCE = ROOT / 'reference-production-80.png'
im = Image.open(SOURCE)
rgba = im.convert('RGBA')
OUT = ROOT / 'blender-v2'
OUT.mkdir(exist_ok=True)
scale, gap, top = 14, 18, 36
sheet = Image.new('RGB', (5 * (16 * scale + gap) + gap, 5 * (16 * scale + top + gap) + top), '#253039')
draw = ImageDraw.Draw(sheet)
stats = []
palette = sorted(set(rgba.getdata()), key=lambda x: (x[3], x[0]))
for row in range(5):
    for col in range(5):
        frame = rgba.crop((col * 16, row * 16, (col + 1) * 16, (row + 1) * 16))
        x0, y0 = gap + col * (16 * scale + gap), top + row * (16 * scale + top + gap)
        draw.text((x0, y0 - 22), f'row {row} / frame {col} / t={col * .2:.1f}', fill='white')
        enlarged = frame.resize((16 * scale, 16 * scale), Image.Resampling.NEAREST)
        sheet.paste(enlarged, (x0, y0), enlarged)
        for n in range(17):
            draw.line((x0+n*scale, y0, x0+n*scale, y0+16*scale), fill='#566068', width=1)
            draw.line((x0, y0+n*scale, x0+16*scale, y0+n*scale), fill='#566068', width=1)
        draw.line((x0+8*scale, y0, x0+8*scale, y0+16*scale), fill='#d09877', width=1)
        draw.line((x0, y0+8*scale, x0+16*scale, y0+8*scale), fill='#d09877', width=1)
        opaque = [(x,y) for y in range(16) for x in range(16) if frame.getpixel((x,y))[3]]
        channels = []
        for color in palette:
            if color[3] == 0:
                continue
            points = [(x,y) for y in range(16) for x in range(16) if frame.getpixel((x,y)) == color]
            channels.append({'rgba':color,'count':len(points),'coordinates':points})
        stats.append({'row':row,'frame':col,'opaque_count':len(opaque),
                      'bbox_exclusive':frame.getbbox(),
                      'silhouette_centroid_not_body_anchor':[round(sum(p[i] for p in opaque)/len(opaque),3) for i in (0,1)],
                      'palette_points':channels})
sheet.save(OUT/'reference-contact-sheet.png')
record={'source':str(SOURCE),'sha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
        'dimensions':im.size,'mode':im.mode,'rgba_palette':palette,
        'runtime_rows_zero_based':[1,3],
        'measurement_boundary':'alpha gives silhouette only; palette values are not semantic body labels. No per-frame registration, recentering, repainting or interpolation.',
        'frames':stats}
(OUT/'reference-measurements.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps({'source':record['source'],'sha256':record['sha256'],'palette':palette,
                  'frames':[{k:v for k,v in s.items() if k!='palette_points'} for s in stats]},indent=2))
