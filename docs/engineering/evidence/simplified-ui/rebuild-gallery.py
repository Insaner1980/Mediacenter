"""Build comparison evidence from immutable designs and Playwright captures."""
from pathlib import Path
from PIL import Image, ImageDraw

out = Path(__file__).resolve().parent
sources = out.parents[2] / 'design/2026-11-06-simplified'
screens = [('home', '01-etusivu.png'), ('audio', '02-musiikki.png'),
           ('settings', '03-asetukset.png'), ('video', '04-videos.png'),
           ('photo', '05-photos.png'), ('drives', '06-drives-usb.png')]
sections = []
for name, original in screens:
    a = Image.open(sources / original).convert('RGB')
    b = Image.open(out / f'{name}-implementation.png').convert('RGB')
    assert a.size == b.size
    Image.blend(a, b, .5).save(out / f'{name}-overlay.jpg', quality=90)
    compare = Image.new('RGB', (a.width * 2, a.height + 30), 'white')
    compare.paste(a, (0, 30))
    compare.paste(b, (a.width, 30))
    draw = ImageDraw.Draw(compare)
    draw.text((10, 8), 'Approved reference', fill='black')
    draw.text((a.width + 10, 8), 'Implementation', fill='black')
    compare.save(out / f'{name}-comparison.jpg', quality=90)
    sections.append(f'<section><h2>{name}</h2><a href="{name}-comparison.jpg"><img src="{name}-comparison.jpg" alt="{name}: reference and implementation"></a><p><a href="{name}-overlay.jpg">50% overlay</a> · <a href="{name}-geometry.json">Measured geometry</a></p></section>')
for theme in ['light', 'dark']:
    files = sorted((out / 'responsive').glob(f'*-{theme}-*.jpg'))
    sheet = Image.new('RGB', (1600, 6 * 270), 'white')
    draw = ImageDraw.Draw(sheet)
    for n, file in enumerate(files):
        im = Image.open(file)
        im.thumbnail((390, 240))
        x, y = n % 4 * 400, n // 4 * 270
        sheet.paste(im, (x, y + 24))
        draw.text((x + 5, y + 5), file.stem, fill='black')
    sheet.save(out / f'responsive-{theme}-overview.jpg', quality=85)
    sections.append(f'<section><h2>{theme} responsive overview</h2><a href="responsive-{theme}-overview.jpg"><img src="responsive-{theme}-overview.jpg" alt="{theme} responsive matrix"></a></section>')
for folder in ['text-zoom', 'states']:
    sections.append(f'<section><h2>{folder}</h2><div class="grid">' + ''.join(f'<a href="{folder}/{f.name}"><img src="{folder}/{f.name}" alt="{f.stem}">{f.stem}</a>' for f in sorted((out / folder).glob('*.jpg'))) + '</div></section>')
sections.append('<section><h2>Native WebKitGTK</h2><div class="grid">' + ''.join(f'<a href="{f.name}"><img src="{f.name}" alt="{f.stem}">{f.stem}</a>' for f in sorted(out.glob('native-*.png'))) + '</div></section>')
(out / 'index.html').write_text('<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Simplified UI verification</title><style>body{margin:24px;background:#f1eddf;color:#191912;font:16px system-ui}img{max-width:100%;height:auto}section{margin:40px 0}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(300px,1fr));gap:20px}a{color:inherit}</style><h1>Simplified UI verification — 6 November 2026</h1><p>Approved references on the left, implementation on the right. Read <a href="acceptance.md">acceptance and limitations</a> and <a href="visual-review.md">visual decisions</a>.</p>' + ''.join(sections) + '</html>')
