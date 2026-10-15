# The home screen's stage as the app draws it: the vinyl Clawd (clawd_cast.frag compiled with "#define MAT_ID 0",
# as live_gl.rs does) and Null, each placed by toys.slint's floor-line rule in its home.slint slot. Renders a few
# moments of the story timelines to D:\Outputs\CallMe\design\live3d\integrated. Usage: python stage.py
import os, json
from render import VERT, HERE, stage, np, Image, moderngl
import render as R

OUT = r'D:\Outputs\CallMe\design\live3d\integrated'
TL = r'D:\Outputs\CallMe\design\live3d\cast\timelines'
S = 2.75
# home.slint: Clawd's slot 230x176 (feet at 0.967), a 4 px gap, Null's 96x172 (feet at 0.975), centred in a row.
SLOTS = {'clawd': (0, 0, 230, 176, 0.967, 300, 250, 0.885), 'null': (234, 2, 96, 172, 0.975, 150, 230, 0.846)}


def program(name, define):
    src = open(os.path.join(HERE, name + '.frag'), encoding='utf8').read()
    version, rest = src.split('\n', 1)
    src = '#version 330\n' + define + '\n' + rest.replace('precision highp float;', '', 1)
    R.render('null', {}, 0.1)  # make the context
    return R._ctx.program(vertex_shader=VERT, fragment_shader=src)


def draw(prog, size, u):
    ctx = R._ctx
    w, h = round(size[0] * S), round(size[1] * S)
    fbo = ctx.simple_framebuffer((w, h), components=4)
    vao = ctx.vertex_array(prog, [])
    prog['uRes'].value = (w, h)
    for k, v in u.items():
        if k in prog:
            prog[k].value = tuple(float(x) for x in v)
    fbo.use()
    fbo.clear(0, 0, 0, 0)
    vao.render(moderngl.TRIANGLES, vertices=3)
    a = np.frombuffer(fbo.read(components=4), np.uint8).reshape(h, w, 4)[::-1].astype(np.float32) / 255
    vao.release()
    fbo.release()
    return a


def composite(uc, un, pick='claude'):
    W, H = 420, 330  # logical: the row plus room above for hops and antenna
    ox, oy = (W - 330) / 2, 90
    img = stage(round(W * S), round(H * S), S)
    for name, u, prog in (('clawd', uc, PC), ('null', un, PN)):
        sx, sy, sw, sh, feet, iw, ih, floor = SLOTS[name]
        a = draw(prog, (iw, ih), u)
        if (name == 'clawd') != (pick == 'claude'):
            a[..., 3] *= 0.55  # the one the Call button doesn't ask for is dimmed
        x = round((ox + sx + (sw - iw) / 2) * S)
        y = round((oy + sy + sh * feet - ih * floor) * S)
        h, w = a.shape[:2]
        reg = img[y:y + h, x:x + w]
        al = a[..., 3:]
        img[y:y + h, x:x + w] = a[..., :3] * al + reg * (1 - al)
    # the floor line both feet should sit on
    return Image.fromarray((np.clip(img, 0, 1) * 255 + 0.5).astype(np.uint8)), round((oy + 176 * 0.967) * S)


if __name__ == '__main__':
    os.makedirs(OUT, exist_ok=True)
    PC, PN = program('clawd_cast', '#define MAT_ID 0'), program('null', '')
    clawd, null = (json.load(open(os.path.join(TL, f'{n}.json'))) for n in ('clawd', 'null'))
    shots = [('idle', clawd['poses']['idle'], null['poses']['idle']),
             ('think', clawd['poses']['think'], null['poses']['think']),
             ('happy', clawd['poses']['happy'], null['poses']['happy']),
             ('sleep', clawd['poses']['sleep'], null['poses']['sleep'])]
    # the highest moment of each hop in the story, to check nothing clips
    top = lambda d: max(d['story'], key=lambda f: f['u']['uBody'][1])['u']
    shots.append(('hop-top', top(clawd), top(null)))
    tiles = []
    for tag, uc, un in shots:
        im, floor = composite(uc, un, 'codex' if tag == 'think' else 'claude')
        im.save(os.path.join(OUT, f'stage-{tag}.png'))
        tiles.append((tag, im, floor))
    im, floor = composite(clawd['poses']['idle'], null['poses']['idle'])
    a = np.asarray(im).copy()
    a[floor, :, :] = (0, 200, 255)
    Image.fromarray(a).save(os.path.join(OUT, 'stage-floor-check.png'))
    w, h = tiles[0][1].size
    sheet = Image.new('RGB', (w * 3, h * 2))
    for i, (_, t, _) in enumerate(tiles):
        sheet.paste(t, ((i % 3) * w, (i // 3) * h))
    sheet.paste(Image.fromarray(a), (2 * w, h))
    sheet.save(os.path.join(OUT, 'stage-sheet.png'))
    print('saved', OUT)
