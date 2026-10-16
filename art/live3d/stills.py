import os, json
from render import VERT, HERE, np, Image, moderngl
import render as R

TL = r'D:\Outputs\CallMe\design\live3d\cast\timelines'
OUT = os.path.join(HERE, '..', '..', 'phone', 'ui', 'faces')
K, SF, HEAD = 0.21, 2.75, 40
SOLO = {'null': 1.2}
FACES = {'clawd': ('clawd', 300, 250), 'haiku': ('haiku', 300, 250), 'sonnet': ('sonnet', 300, 250), 'opus': ('opus', 300, 250), 'null': ('null', 150, 230)}

if __name__ == '__main__':
    os.makedirs(OUT, exist_ok=True)
    R.render('null', {}, 0.1)
    ctx = R._ctx
    for face, (tl, fw, fh) in FACES.items():
        data = json.load(open(os.path.join(TL, tl + '.json')))
        prog = R.program(data['shader'])
        k = K * SOLO.get(face, 1)
        w, h, vh = round(fw * k * SF), round(fh * k * SF), round((fh + HEAD) * k * SF)
        S = 4
        fbo = ctx.simple_framebuffer((w * S, vh * S), components=4)
        vao = ctx.vertex_array(prog, [])
        prog['uRes'].value = (w * S, h * S)
        for k, v in data['poses']['idle'].items():
            if k in prog:
                prog[k].value = tuple(float(x) for x in v)
        fbo.use()
        fbo.clear(0, 0, 0, 0)
        vao.render(moderngl.TRIANGLES, vertices=3)
        a = np.frombuffer(fbo.read(components=4), np.uint8).reshape(vh * S, w * S, 4)[::-1].astype(np.float32) / 255
        a[..., :3] *= a[..., 3:]
        a = a.reshape(vh, S, w, S, 4).mean(axis=(1, 3))
        a[..., :3] /= np.maximum(a[..., 3:], 1e-6)
        Image.fromarray((np.clip(a, 0, 1) * 255 + 0.5).astype(np.uint8), 'RGBA').save(os.path.join(OUT, face + '.png'), optimize=True)
        vao.release()
        fbo.release()
        print(face, w, vh)
