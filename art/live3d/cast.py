# Renders the cast from the motion timelines (motion/ -> cast-motion <dir>): for each character a pose sheet, a
# side-by-side with its reference art, the 27 s story as a 30 fps MP4 and a contact strip, and a frame-to-frame
# jump report. Usage: python cast.py [names...]   (default: all)
import sys, os, json, subprocess
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from render import render, over, stage, OUT, SIZE, np, Image
from refs import ref
from PIL import ImageDraw, ImageFont

FFMPEG = r'B:\Tools\MediaTools\bin\ffmpeg.exe'
SCALE = 2.75  # a phone's device pixels per logical pixel
TL = os.path.join(OUT, 'timelines')
NAMES = ['null', 'haiku', 'sonnet', 'opus', 'clawd']
try:
    FONT = ImageFont.truetype('segoeui.ttf', 22)
except OSError:
    FONT = ImageFont.load_default()


def frame(shader, u, bg):
    return over(render(shader, u, SCALE), bg)


def label(im, text):
    d = ImageDraw.Draw(im)
    d.text((14, 10), text, fill=(150, 150, 160), font=FONT)
    return im


def poses_sheet(name, data, bg):
    order = ['idle', 'blink', 'think', 'talk', 'happy', 'sleep']
    ims = [label(frame(data['shader'], data['poses'][k], bg), k) for k in order]
    w, h = ims[0].size
    sheet = Image.new('RGB', (w * 3, h * 2))
    for i, im in enumerate(ims):
        sheet.paste(im, ((i % 3) * w, (i // 3) * h))
    sheet.save(os.path.join(OUT, f'{name}-poses.png'))


def side_by_side(name, data, bg):
    rgba = render(data['shader'], data['poses']['idle'], SCALE)
    ys, xs = np.where(rgba[..., 3] > 0.9)
    pad = 30
    im = over(rgba, bg).crop((max(xs.min() - pad, 0), max(ys.min() - pad, 0), min(xs.max() + pad, rgba.shape[1]), min(ys.max() + pad, rgba.shape[0])))
    H = 640
    im = im.resize((round(im.width * H / im.height), H), Image.LANCZOS)
    r = ref(name, H)
    out = Image.new('RGB', (r.width + im.width + 10, H), (0x11, 0x12, 0x14))
    out.paste(r, (0, 0))
    out.paste(im, (r.width + 10, 0))
    label(out, 'reference')
    ImageDraw.Draw(out).text((r.width + 24, 10), 'live (offline render, idle)', fill=(150, 150, 160), font=FONT)
    out.save(os.path.join(OUT, f'{name}-vs-ref.png'))


def story(name, data, bg):
    frames = data['story']
    w, h = (round(v * SCALE) for v in SIZE[data['shader']])
    w, h = w - w % 2, h - h % 2
    mp4 = os.path.join(OUT, f'{name}-story-30fps.mp4')
    ff = subprocess.Popen([FFMPEG, '-y', '-loglevel', 'error', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-s', f'{w}x{h}', '-r', '30', '-i', '-',
                           '-c:v', 'libx264', '-pix_fmt', 'yuv420p', '-crf', '16', '-preset', 'slow', mp4], stdin=subprocess.PIPE)
    prev, diffs, strip = None, [], []
    for i, f in enumerate(frames):
        im = frame(data['shader'], f['u'], bg).crop((0, 0, w, h))
        a = np.asarray(im, np.float32)
        if prev is not None:
            diffs.append(float(np.abs(a - prev).mean()))
        prev = a
        ff.stdin.write(im.tobytes())
        if i % 15 == 0:
            strip.append(label(im.resize((w // 3, h // 3), Image.LANCZOS), f"{f['t']:.1f}s"))
    ff.stdin.close()
    ff.wait()
    cols = 9
    tw, th = strip[0].size
    sheet = Image.new('RGB', (tw * cols, th * ((len(strip) + cols - 1) // cols)), (0x11, 0x12, 0x14))
    for i, im in enumerate(strip):
        sheet.paste(im, ((i % cols) * tw, (i // cols) * th))
    sheet.save(os.path.join(OUT, f'{name}-story-strip.png'))
    d = np.array(diffs)
    med = float(np.median(d))
    worst = np.argsort(d)[-5:][::-1]
    print(f'{name}: {len(frames)} frames, mean change/frame median {med:.2f}, max {d.max():.2f} at '
          + ', '.join(f"{frames[k + 1]['t']:.2f}s ({d[k]:.2f})" for k in worst))
    return d


if __name__ == '__main__':
    names = sys.argv[1:] or NAMES
    for name in names:
        data = json.load(open(os.path.join(TL, f'{name}.json')))
        w, h = (round(v * SCALE) for v in SIZE[data['shader']])
        bg = stage(w, h, SCALE)
        poses_sheet(name, data, bg)
        side_by_side(name, data, bg)
        story(name, data, bg)
