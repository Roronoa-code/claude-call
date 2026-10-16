import os
from PIL import Image
D = r'D:\Outputs\CallMe\design\characters'
REFS = {
    'clawd': (os.path.join(D, 'style-02-vinyl.png'), (30, 470, 720, 1090)),
    'haiku': (os.path.join(D, 'style-04-cartoon.png'), (30, 470, 720, 1090)),
    'sonnet': (os.path.join(D, 'style-01-plush.png'), (10, 470, 730, 1150)),
    'opus': (os.path.join(D, 'style-07-chrome.png'), (30, 500, 740, 1100)),
    'null': (os.path.join(D, 'vinyl', 'null-idle.png'), (262, 100, 762, 900)),
}


def ref(name, height):
    path, box = REFS[name]
    im = Image.open(path).convert('RGBA').crop(box)
    bg = Image.new('RGBA', im.size, (0x11, 0x12, 0x14, 255))
    bg.alpha_composite(im)
    return bg.convert('RGB').resize((round(im.width * height / im.height), height), Image.LANCZOS)
