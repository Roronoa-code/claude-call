# PREVIEW (branch preview/chats): labelled side-by-side comparisons of today's chats and the four directions, from
# chat_capture.py's stills. Usage: python chat_compare.py
import os
from PIL import Image, ImageDraw, ImageFont

OUT = r'D:\Outputs\CallMe\design\step5-chats'
COLS = [('0-current', 'Current'), ('1-stage', '1  Stage'), ('2-beside', '2  Beside'), ('3-script', '3  Script'), ('4-light', '4  In the light')]
SHEETS = {
    'comparison-list': '01-list-unread',
    'comparison-thread': '12-answer-no-keyboard',
    'comparison-thinking': '09-thinking',
    'comparison-new-chat': '17-new-chat',
}
font = ImageFont.truetype(r'C:\Windows\Fonts\segoeuib.ttf', 46)
W, H, GAP, TOP = 540, 1200, 28, 92
for name, still in SHEETS.items():
    sheet = Image.new('RGB', (len(COLS) * W + (len(COLS) + 1) * GAP, H + TOP + GAP), (18, 18, 20))
    d = ImageDraw.Draw(sheet)
    for i, (folder, label) in enumerate(COLS):
        x = GAP + i * (W + GAP)
        im = Image.open(os.path.join(OUT, folder, still + '.png')).convert('RGB').resize((W, H), Image.LANCZOS)
        sheet.paste(im, (x, TOP))
        tw = d.textlength(label, font=font)
        d.text((x + (W - tw) / 2, 18), label, font=font, fill=(236, 233, 226) if i else (150, 150, 155))
    sheet.save(os.path.join(OUT, name + '.png'), optimize=True)
    sheet.save(os.path.join(OUT, name + '.jpg'), quality=90)
    print(name, sheet.size)

# crops of the details, per direction: a list row, the words, thinking, the field
CROPS = {
    'crop-list': ('01-list-unread', (0, 140, 1080, 1060)),
    'crop-words': ('12-answer-no-keyboard', (0, 1060, 1080, 2000)),
    'crop-thinking': ('09-thinking', (0, 140, 1080, 1180)),
    'crop-new-chat': ('17-new-chat', (0, 300, 1080, 1700)),
}
for folder, _ in COLS[1:]:
    for name, (still, box) in CROPS.items():
        Image.open(os.path.join(OUT, folder, still + '.png')).crop(box).save(os.path.join(OUT, folder, name + '.png'))
print('crops done')
