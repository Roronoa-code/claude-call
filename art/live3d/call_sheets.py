# PREVIEW (branch preview/call-screen): after call_capture.py has run for every direction, trims each call.mp4 to start
# as the call screen appears (the capture cold-starts the app), remakes the 540 px copies, saves the crops, and draws
# the labelled comparisons (talking and ringing) in D:\Outputs\CallMe\design\step4-call.
# Usage: python call_sheets.py [direction folder ...]   (only those were captured again; default all)
import os, subprocess
import numpy as np
from PIL import Image, ImageDraw, ImageFont

OUT = r'D:\Outputs\CallMe\design\step4-call'
LOOKS = [('0-current', 'Current'), ('1-stage', '1  Stage'), ('2-closeup', '2  Close-up'), ('3-comic', '3  Comic'), ('4-lyrics', '4  Lyrics')]
CF = 0x08000000
# Crops (px of the 1080 x 2400 screenshots): the caller and its stage while talking, the thinking sign, the controls.
CROPS = {
    '0-current': dict(caller=('2-talking', (0, 240, 1080, 1100)), thinking=('5b-thinking', (0, 240, 1080, 1100)), controls=('3-listening', (0, 1540, 1080, 2330))),
    '1-stage': dict(caller=('2-talking', (0, 300, 1080, 1460)), thinking=('5b-thinking', (0, 300, 1080, 1460)), controls=('3-listening', (0, 1560, 1080, 2330))),
    '2-closeup': dict(caller=('2-talking', (0, 140, 1080, 1500)), thinking=('5b-thinking', (0, 140, 1080, 2000)), controls=('3-listening', (0, 1700, 1080, 2330))),
    '3-comic': dict(caller=('2-talking', (0, 580, 1080, 1620)), thinking=('5b-thinking', (0, 580, 1080, 1800)), controls=('3-listening', (0, 1350, 1080, 2330))),
    '4-lyrics': dict(caller=('2-talking', (0, 130, 1080, 1700)), thinking=('5b-thinking', (0, 130, 1080, 1700)), controls=('3-listening', (0, 1780, 1080, 2330))),
}


def first_screen(vid):
    """Seconds into the recording just before the caller fades in on the call screen (after the launcher and splash)."""
    p = subprocess.run(['ffmpeg', '-loglevel', 'error', '-i', vid, '-vf', 'fps=10,scale=54:120', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'],
                       capture_output=True, creationflags=CF).stdout
    frames = np.frombuffer(p, np.uint8).reshape(-1, 120, 54, 3).astype(int)
    for i, f in enumerate(frames):
        orange = ((f[..., 0] > 170) & (f[..., 1] > 50) & (f[..., 1] < 140) & (f[..., 2] < 60)).sum()
        if orange > 15:
            return max(0.0, i / 10 - 0.3)
    return 0.0


def videos(d):
    src = os.path.join(OUT, d, 'call.mp4')
    raw = os.path.join(OUT, d, 'call-raw.mp4')
    if not os.path.exists(raw):
        os.replace(src, raw)
    t0 = first_screen(raw)
    subprocess.run(['ffmpeg', '-y', '-loglevel', 'error', '-ss', f'{t0:.2f}', '-i', raw, '-c:v', 'libx264', '-crf', '16', '-preset', 'slow',
                    '-pix_fmt', 'yuv420p', '-an', src], creationflags=CF)
    subprocess.run(['ffmpeg', '-y', '-loglevel', 'error', '-i', src, '-vf', 'scale=540:-2', '-c:v', 'libx264', '-crf', '26', '-preset', 'slow',
                    '-pix_fmt', 'yuv420p', '-an', os.path.join(OUT, d, 'call-small.mp4')], creationflags=CF)
    os.remove(raw)
    print(d, 'trimmed at', t0)


def crops(d):
    for name, (shot, box) in CROPS[d].items():
        Image.open(os.path.join(OUT, d, shot + '.png')).convert('RGB').crop(box).save(os.path.join(OUT, d, f'crop-{name}.png'))


def comparison(shot, name):
    w, gap, top = 540, 28, 96
    h = w * 2400 // 1080
    out = Image.new('RGB', (len(LOOKS) * w + (len(LOOKS) + 1) * gap, h + top + gap), (10, 9, 12))
    font = ImageFont.truetype(r'C:\Windows\Fonts\segoeuib.ttf', 44)
    dr = ImageDraw.Draw(out)
    for i, (d, label) in enumerate(LOOKS):
        x = gap + i * (w + gap)
        im = Image.open(os.path.join(OUT, d, shot + '.png')).convert('RGB').resize((w, h), Image.LANCZOS)
        out.paste(im, (x, top))
        tw = dr.textlength(label, font=font)
        dr.text((x + (w - tw) / 2, 22), label, font=font, fill=(242, 242, 243) if i else (154, 156, 163))
    out.save(os.path.join(OUT, name + '.png'))
    out.save(os.path.join(OUT, name + '.jpg'), quality=90)


if __name__ == '__main__':
    import sys
    only = sys.argv[1:]  # the directions just captured again (default: all)
    for d, _ in LOOKS:
        if only and d not in only:
            continue
        p = os.path.join(OUT, d)
        if os.path.exists(os.path.join(p, '1b-ringing.png')):  # the first ringing shot catches the app still starting
            os.replace(os.path.join(p, '1b-ringing.png'), os.path.join(p, '1-ringing.png'))
        if os.path.exists(os.path.join(p, '2a-answer-mid.png')):
            os.replace(os.path.join(p, '2a-answer-mid.png'), os.path.join(p, '2a-answering-mid-animation.png'))
        videos(d)
        crops(d)
    comparison('2-talking', 'comparison')
    comparison('1-ringing', 'comparison-ringing')
    print('done')
