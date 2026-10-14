"""Generate Windows ICO and window PNG from assets/img/logo.png (requires Pillow).
Generated assets are included; normal Cargo builds do not require Python.
"""
from pathlib import Path
from PIL import Image

root = Path(__file__).resolve().parent.parent
source = Image.open(root/'assets/img/logo.png').convert('RGBA')
if source.width != source.height:
    raise ValueError('Application icon source must be square')
source.save(root/'assets/img/xpeed.ico', sizes=[(n,n) for n in (16,24,32,48,64,128,256)])
source.resize((256,256), Image.Resampling.LANCZOS).save(root/'assets/img/app-icon.png')
