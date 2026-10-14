# Icon sources

`icon.svg`, `tray.svg` and `tray-missed.svg` are the masters for every file in
`src-tauri/icons`. To regenerate the PNG, ICO and ICNS files after editing them:

```sh
pip install cairosvg pillow
cd src-tauri/icons
python3 - <<'PY'
import cairosvg, io
from PIL import Image
def r(svg, n):
    return Image.open(io.BytesIO(cairosvg.svg2png(url=svg, output_width=n, output_height=n))).convert('RGBA')
for n, f in ((1024, 'icon.png'), (32, '32x32.png'), (128, '128x128.png'), (256, '128x128@2x.png')):
    r('source/icon.svg', n).save(f, optimize=True)
r('source/icon.svg', 256).save('icon.ico', sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
r('source/icon.svg', 1024).save('icon.icns')
r('source/tray.svg', 64).save('tray.png', optimize=True)
r('source/tray-missed.svg', 64).save('tray-missed.png', optimize=True)
PY
```
